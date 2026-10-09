use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    FFMPEG_VERSION,
    profiles::{EncodingSpeed, MediaKind, OutputFormat, Profile, RateControl},
};

#[derive(Clone)]
pub struct Engine {
    pub state: Arc<Mutex<QueueState>>,
    wake: mpsc::Sender<()>,
    shutdown: Arc<AtomicBool>,
    worker: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

#[derive(Default)]
pub struct QueueState {
    pub jobs: VecDeque<Job>,
}

#[derive(Clone)]
pub struct Job {
    pub id: Uuid,
    pub source: PathBuf,
    pub profile: Profile,
    pub status: JobStatus,
    pub progress: Option<f32>,
    pub remaining_seconds: Option<u64>,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
pub enum JobStatus {
    Queued,
    Running,
    Completed(PathBuf),
    Failed(String),
    Cancelled,
}

impl JobStatus {
    pub fn active(&self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

impl Engine {
    pub fn new() -> Self {
        let (wake, receiver) = mpsc::channel();
        let engine = Self {
            state: Arc::new(Mutex::new(QueueState::default())),
            wake,
            shutdown: Arc::new(AtomicBool::new(false)),
            worker: Arc::new(Mutex::new(None)),
        };
        let worker = engine.clone();
        *engine.worker.lock().unwrap() = Some(thread::spawn(move || worker.work(receiver)));
        engine
    }

    pub fn enqueue(&self, files: Vec<PathBuf>, profile: Profile) -> Result<()> {
        profile.validate()?;
        ensure!(
            !files.is_empty() && files.len() <= 1000,
            "Select between 1 and 1000 files"
        );
        let mut validated = Vec::new();
        for file in files {
            ensure!(
                file.is_absolute() && file.is_file(),
                "Not a local file: {}",
                file.display()
            );
            ensure!(
                MediaKind::for_path(&file) == Some(profile.format.kind()),
                "{} is incompatible with {}",
                file.display(),
                profile.format.label()
            );
            validated.push(file);
        }
        let mut state = self.state.lock().unwrap();
        ensure!(
            state.jobs.iter().filter(|j| j.status.active()).count() + validated.len() <= 1000,
            "The queue is full (1000 files maximum)"
        );
        for source in validated {
            state.jobs.push_back(Job {
                id: Uuid::new_v4(),
                source,
                profile: profile.clone(),
                status: JobStatus::Queued,
                progress: None,
                remaining_seconds: None,
                cancel: Arc::new(AtomicBool::new(false)),
            });
        }
        drop(state);
        self.wake
            .send(())
            .context("Conversion worker is unavailable")?;
        Ok(())
    }

    pub fn cancel(&self, id: Uuid) {
        let mut state = self.state.lock().unwrap();
        if let Some(job) = state
            .jobs
            .iter_mut()
            .find(|job| job.id == id && job.status.active())
        {
            job.cancel.store(true, Ordering::Relaxed);
            if matches!(job.status, JobStatus::Queued) {
                job.status = JobStatus::Cancelled;
            }
        }
    }

    pub fn clear_finished(&self) {
        self.state
            .lock()
            .unwrap()
            .jobs
            .retain(|job| job.status.active());
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let mut state = self.state.lock().unwrap();
        for job in &mut state.jobs {
            if job.status.active() {
                job.cancel.store(true, Ordering::Relaxed);
                if matches!(job.status, JobStatus::Queued) {
                    job.status = JobStatus::Cancelled;
                }
            }
        }
        let _ = self.wake.send(());
    }

    pub fn stop_and_wait(&self) {
        self.stop();
        let handle = self.worker.lock().unwrap().take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }

    fn update(&self, id: Uuid, update: impl FnOnce(&mut Job)) {
        if let Some(job) = self
            .state
            .lock()
            .unwrap()
            .jobs
            .iter_mut()
            .find(|job| job.id == id)
        {
            update(job);
        }
    }

    fn work(&self, receiver: mpsc::Receiver<()>) {
        while receiver.recv().is_ok() {
            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }
            loop {
                let job = {
                    let mut state = self.state.lock().unwrap();
                    let job = state
                        .jobs
                        .iter_mut()
                        .find(|j| matches!(j.status, JobStatus::Queued));
                    job.map(|job| {
                        job.status = JobStatus::Running;
                        job.clone()
                    })
                };
                let Some(job) = job else {
                    break;
                };
                let result = self.convert(&job);
                self.update(job.id, |item| {
                    item.status = match result {
                        Ok(output) => {
                            item.progress = Some(1.0);
                            JobStatus::Completed(output)
                        }
                        Err(_) if job.cancel.load(Ordering::Relaxed) => JobStatus::Cancelled,
                        Err(error) => JobStatus::Failed(format!("{error:#}")),
                    };
                    item.remaining_seconds = None;
                });
                let mut state = self.state.lock().unwrap();
                // ponytail: retain the latest 20 finished jobs in memory; persist history only if requested.
                while state.jobs.iter().filter(|j| !j.status.active()).count() > 20 {
                    if let Some(index) = state.jobs.iter().position(|j| !j.status.active()) {
                        state.jobs.remove(index);
                    }
                }
                if self.shutdown.load(Ordering::Relaxed) {
                    break;
                }
            }
        }
    }

    fn convert(&self, job: &Job) -> Result<PathBuf> {
        let tools = Tools::bundled()?;
        tools.verify()?;
        let media = tools.probe(&job.source, job.profile.format.kind(), &job.cancel)?;
        let stream = media
            .streams
            .iter()
            .find(|s| s.codec_type.as_deref() == Some("video"))
            .context("The file contains no image or video stream")?;
        if job.profile.format.kind() == MediaKind::Image {
            ensure!(
                stream.nb_read_frames.as_deref() == Some("1"),
                "Animated images are not supported in this version"
            );
        }
        let parent = job
            .source
            .parent()
            .context("The source file has no parent directory")?;
        let temporary = tempfile::Builder::new()
            .prefix(".mue-")
            .suffix(&format!(".{}", job.profile.format.extension()))
            .tempfile_in(parent)?;
        let duration = media
            .format
            .and_then(|f| f.duration)
            .and_then(|d| d.parse::<f64>().ok())
            .filter(|d| d.is_finite() && *d > 0.0);
        let fps = stream.avg_frame_rate.as_deref().and_then(parse_rate);
        let passes = if job.profile.format.kind() == MediaKind::Video
            && job.profile.rate_control == RateControl::VbrTwoPass
        {
            2
        } else {
            1
        };
        // A private directory owns all encoder-specific sidecars, including on cancellation.
        let statistics = if passes == 2 {
            Some(
                tempfile::Builder::new()
                    .prefix(".mue-pass-")
                    .tempdir_in(parent)?,
            )
        } else {
            None
        };
        let started = Instant::now();
        for pass in 1..=passes {
            let mut command = tools.command("ffmpeg");
            command
                .args([
                    "-hide_banner",
                    "-nostdin",
                    "-loglevel",
                    "error",
                    "-nostats",
                    "-progress",
                    "pipe:1",
                    "-y",
                    "-i",
                ])
                .arg(&job.source);
            add_encoding_args(&mut command, &job.profile, fps, passes == 2 && pass == 1);
            if let Some(statistics) = &statistics {
                command.args(["-pass", &pass.to_string()]);
                command
                    .arg(if job.profile.format == OutputFormat::Mp4H265 {
                        "-x265-stats"
                    } else {
                        "-passlogfile"
                    })
                    .arg(statistics.path().join("stats"));
            }
            if passes == 2 && pass == 1 {
                command.args(["-f", "null", "-"]);
            } else {
                command.arg(temporary.path());
            }
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
            ensure!(!job.cancel.load(Ordering::Relaxed), "Conversion cancelled");
            let mut child = command.spawn().context("Cannot start FFmpeg")?;
            let stderr = child.stderr.take().unwrap();
            let log = thread::spawn(move || read_log(stderr));
            let stdout = child.stdout.take().unwrap();
            let (sender, receiver) = mpsc::channel();
            let progress = thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(value) = line
                        .strip_prefix("out_time_us=")
                        .and_then(|s| s.parse::<f64>().ok())
                    {
                        let _ = sender.send(value / 1_000_000.0);
                    }
                }
            });
            let status = loop {
                if job.cancel.load(Ordering::Relaxed) {
                    let _ = child.kill();
                }
                for seconds in receiver.try_iter() {
                    if let Some(duration) =
                        duration.filter(|_| job.profile.format.kind() == MediaKind::Video)
                    {
                        let fraction = ((f64::from(pass - 1)
                            + (seconds / duration).clamp(0.0, 1.0))
                            / f64::from(passes))
                        .clamp(0.0, 0.99) as f32;
                        self.update(job.id, |item| {
                            item.progress = Some(fraction);
                            item.remaining_seconds =
                                (fraction > 0.01 && started.elapsed().as_secs() >= 2).then(|| {
                                    (started.elapsed().as_secs_f32() * (1.0 - fraction) / fraction)
                                        as u64
                                });
                        });
                    }
                }
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => thread::sleep(Duration::from_millis(100)),
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(error.into());
                    }
                }
            };
            let _ = progress.join();
            let log = log.join().unwrap_or_default();
            ensure!(!job.cancel.load(Ordering::Relaxed), "Conversion cancelled");
            ensure!(
                status.success(),
                "FFmpeg failed (pass {pass}/{passes}): {}",
                log.trim()
            );
        }
        ensure!(
            temporary.as_file().metadata()?.len() > 0,
            "FFmpeg produced an empty file"
        );
        temporary.as_file().sync_all()?;
        let template = crate::naming::FilenameTemplate::parse(&job.profile.filename_template)?;
        let output_dimensions = if template.needs_output_dimensions() {
            // Inspect the encoded output rather than approximating scaling or rotation rules.
            let output = tools.probe(temporary.path(), MediaKind::Video, &job.cancel)?;
            Some(
                output
                    .streams
                    .iter()
                    .find(|stream| stream.codec_type.as_deref() == Some("video"))
                    .context("The converted file contains no image or video stream")?
                    .dimensions()
                    .context("The converted file has no valid dimensions")?,
            )
        } else {
            None
        };
        let name = template.render(
            &job.source,
            job.profile.format.extension(),
            &job.profile.name,
            stream.dimensions().as_ref(),
            output_dimensions.as_ref(),
        )?;
        // Cancellation and publication have one commit point. Once published, a job cannot be cancelled.
        let mut state = self.state.lock().unwrap();
        ensure!(!job.cancel.load(Ordering::Relaxed), "Conversion cancelled");
        let output = publish(temporary, &job.source, &name)?;
        if let Some(item) = state.jobs.iter_mut().find(|item| item.id == job.id) {
            item.status = JobStatus::Completed(output.clone());
            item.progress = Some(1.0);
        }
        Ok(output)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Tools {
    pub directory: PathBuf,
}

impl Tools {
    pub fn bundled() -> Result<Self> {
        let executable = std::env::current_exe()?;
        Ok(Self {
            directory: executable
                .parent()
                .context("Missing executable directory")?
                .join("ffmpeg"),
        })
    }

    pub fn command(&self, name: &str) -> Command {
        let mut command = Command::new(
            self.directory
                .join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
        );
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
    }

    pub fn verify(&self) -> Result<()> {
        for name in ["ffmpeg", "ffprobe"] {
            let output = self
                .command(name)
                .arg("-version")
                .output()
                .with_context(|| {
                    format!(
                        "Missing bundled {name}. Run scripts/bundle-ffmpeg.ps1 before starting Mue."
                    )
                })?;
            let version = String::from_utf8_lossy(&output.stdout);
            ensure!(
                output.status.success()
                    && version.split_whitespace().nth(2).is_some_and(
                        |v| v == FFMPEG_VERSION || v.starts_with(&format!("{FFMPEG_VERSION}-"))
                    ),
                "Bundled {name} must be version {FFMPEG_VERSION}"
            );
        }
        Ok(())
    }

    fn probe(&self, path: &Path, kind: MediaKind, cancel: &AtomicBool) -> Result<Probe> {
        let mut command = self.command("ffprobe");
        command.args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:stream=codec_type,nb_read_frames,avg_frame_rate,width,height",
            "-of",
            "json",
        ]);
        if kind == MediaKind::Image {
            command.args(["-count_frames", "-read_intervals", "%+#2"]);
        }
        let mut child = command
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let output = thread::spawn(move || {
            let mut bytes = Vec::new();
            BufReader::new(stdout)
                .take(1_048_576)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let log = thread::spawn(move || read_log(stderr));
        let started = Instant::now();
        let status = loop {
            if cancel.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(30) {
                let _ = child.kill();
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            thread::sleep(Duration::from_millis(100));
        };
        let bytes = output
            .join()
            .map_err(|_| anyhow::anyhow!("Probe reader failed"))??;
        let log = log.join().unwrap_or_default();
        ensure!(!cancel.load(Ordering::Relaxed), "Conversion cancelled");
        ensure!(status.success(), "Cannot inspect media: {}", log.trim());
        serde_json::from_slice(&bytes).context("Invalid ffprobe response")
    }
}

#[derive(Deserialize)]
struct Probe {
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}
#[derive(Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    nb_read_frames: Option<String>,
    avg_frame_rate: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

impl ProbeStream {
    fn dimensions(&self) -> Option<crate::naming::Dimensions> {
        let width = self.width.filter(|value| *value > 0)?;
        let height = self.height.filter(|value| *value > 0)?;
        Some(crate::naming::Dimensions { width, height })
    }
}
#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

fn parse_rate(value: &str) -> Option<f64> {
    let (n, d) = value.split_once('/')?;
    let result = n.parse::<f64>().ok()? / d.parse::<f64>().ok()?;
    (result.is_finite() && result > 0.0).then_some(result)
}

fn read_log(mut reader: impl Read) -> String {
    let mut tail = Vec::new();
    let mut buffer = [0; 4096];
    while let Ok(count) = reader.read(&mut buffer) {
        if count == 0 {
            break;
        }
        tail.extend_from_slice(&buffer[..count]);
        if tail.len() > 65536 {
            tail.drain(..tail.len() - 65536);
        }
    }
    String::from_utf8_lossy(&tail).into_owned()
}

fn add_encoding_args(
    command: &mut Command,
    profile: &Profile,
    source_fps: Option<f64>,
    analysis_pass: bool,
) {
    let width = profile
        .max_width
        .map_or("iw".into(), |w| format!("min(iw\\,{w})"));
    let height = profile
        .max_height
        .map_or("ih".into(), |h| format!("min(ih\\,{h})"));
    let mut filter = format!("scale=w='{width}':h='{height}':force_original_aspect_ratio=decrease");
    if profile.format.kind() == MediaKind::Video {
        filter.push_str(":force_divisible_by=2:reset_sar=1");
        if let Some(fps) = profile.max_fps {
            if source_fps.is_some_and(|rate| rate > fps as f64) {
                filter.push_str(&format!(",fps={fps}"));
            }
        }
    }
    if profile.format == OutputFormat::Jpg {
        command.args(["-filter_complex", &format!("[0:v:0]{filter},format=rgba,split=2[foreground][base];[base]drawbox=color=0x{}:t=fill:replace=1[background];[background][foreground]overlay=shortest=1:format=auto,format=yuvj420p[out]", profile.jpeg_background), "-map", "[out]"]);
    } else {
        command.args(["-map", "0:v:0", "-vf", &filter]);
    }
    if !profile.keep_metadata {
        command.args(["-map_metadata", "-1"]);
    }
    match profile.format {
        OutputFormat::Jpg => {
            let quality = 2 + (100 - profile.quality as u32) * 29 / 99;
            command.args([
                "-frames:v",
                "1",
                "-c:v",
                "mjpeg",
                "-q:v",
                &quality.to_string(),
                "-update",
                "1",
            ]);
        }
        OutputFormat::Png => {
            command.args([
                "-frames:v",
                "1",
                "-c:v",
                "png",
                "-compression_level",
                &profile.png_compression.to_string(),
                "-update",
                "1",
            ]);
        }
        OutputFormat::Webp => {
            command.args([
                "-frames:v",
                "1",
                "-c:v",
                "libwebp",
                "-quality",
                &profile.quality.to_string(),
            ]);
        }
        OutputFormat::Mp4 | OutputFormat::Mp4H265 | OutputFormat::Webm => {
            let mp4 = profile.format.is_mp4();
            let h265 = profile.format == OutputFormat::Mp4H265;
            command.args([
                "-c:v",
                if h265 {
                    "libx265"
                } else if mp4 {
                    "libx264"
                } else {
                    "libvpx-vp9"
                },
                "-pix_fmt",
                "yuv420p",
            ]);
            if profile.rate_control == RateControl::Crf {
                command.args(["-crf", &profile.video_crf.to_string()]);
                if !mp4 {
                    command.args(["-b:v", "0"]);
                }
            } else {
                command.args(["-b:v", &format!("{}k", profile.video_bitrate_kbps)]);
            }
            let maximum = if profile.rate_control == RateControl::Cbr {
                Some(profile.video_bitrate_kbps)
            } else {
                profile.max_video_bitrate_kbps
            };
            if let Some(maximum) = maximum {
                command.args([
                    "-maxrate",
                    &format!("{maximum}k"),
                    "-bufsize",
                    &format!("{}k", u64::from(maximum) * 2),
                ]);
            }
            if profile.rate_control == RateControl::Cbr {
                command.args(["-minrate", &format!("{}k", profile.video_bitrate_kbps)]);
                if h265 {
                    command.args(["-x265-params", "strict-cbr=1"]);
                } else if mp4 {
                    // MP4 cannot signal nal-hrd=cbr; use VBV regulation without HRD padding.
                    command.args(["-x264-params", "nal-hrd=vbr"]);
                }
            }
            if h265 && !analysis_pass {
                command.args(["-tag:v", "hvc1"]);
            }
            if mp4 {
                command.args([
                    "-preset",
                    match profile.encoding_speed {
                        EncodingSpeed::Fast => "veryfast",
                        EncodingSpeed::Balanced => "medium",
                        EncodingSpeed::Slow => "slow",
                    },
                ]);
                if !analysis_pass {
                    command.args(["-movflags", "+faststart"]);
                }
            } else {
                command.args([
                    "-deadline",
                    "good",
                    "-cpu-used",
                    match profile.encoding_speed {
                        EncodingSpeed::Fast => "4",
                        EncodingSpeed::Balanced => "2",
                        EncodingSpeed::Slow => "0",
                    },
                ]);
            }
            if profile.keep_audio && !analysis_pass {
                command.args([
                    "-map",
                    "0:a:0?",
                    "-c:a",
                    if mp4 { "aac" } else { "libopus" },
                    "-b:a",
                    &format!("{}k", profile.audio_bitrate_kbps),
                ]);
            } else {
                command.arg("-an");
            }
        }
    }
}

fn publish(
    mut temporary: tempfile::NamedTempFile,
    source: &Path,
    output_name: &std::ffi::OsStr,
) -> Result<PathBuf> {
    let output_path = Path::new(output_name);
    let stem = output_path.file_stem().context("Missing output filename")?;
    let extension = output_path
        .extension()
        .context("Missing output extension")?;
    let parent = source.parent().unwrap();
    for index in 0..10000 {
        let mut name = stem.to_os_string();
        if index > 0 {
            let suffix = format!(" ({index})");
            let budget =
                255 - suffix.len() - 1 - extension.to_string_lossy().encode_utf16().count();
            if stem.to_string_lossy().encode_utf16().count() > budget {
                let text = stem.to_string_lossy();
                let mut units = 0;
                name = text
                    .chars()
                    .take_while(|character| {
                        units += character.len_utf16();
                        units <= budget
                    })
                    .collect::<String>()
                    .into();
            }
            name.push(suffix);
        }
        name.push(".");
        name.push(extension);
        crate::naming::validate_filename(&name)?;
        let destination = parent.join(name);
        if destination == source || destination.exists() {
            continue;
        }
        match temporary.persist_noclobber(&destination) {
            Ok(_) => return Ok(destination),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                temporary = error.file;
            }
            Err(error) => return Err(error).context("Cannot publish converted file"),
        }
    }
    bail!("Cannot find an available output filename")
}

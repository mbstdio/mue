use std::{
    io::Read,
    path::Path,
    process::Stdio,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use gpui::{Image, ImageFormat};
use mue_core::conversion::Tools;

/// Produce a bounded, static thumbnail using the already bundled conversion tools.
pub(crate) fn thumbnail(path: &Path) -> Option<Arc<Image>> {
    let tools = Tools::bundled().ok()?;
    let mut child = tools
        .command("ffmpeg")
        .args(["-nostdin", "-v", "error", "-threads", "1", "-i"])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=96:72:force_original_aspect_ratio=decrease",
            "-filter_threads",
            "1",
            "-threads",
            "1",
            "-c:v",
            "png",
            "-f",
            "image2pipe",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(1_048_576).read_to_end(&mut bytes).ok()?;
        Some(bytes)
    });
    let started = Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if started.elapsed() < Duration::from_secs(10) => {
                thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let bytes = reader.join().ok()??;
    (success && !bytes.is_empty()).then(|| Arc::new(Image::from_bytes(ImageFormat::Png, bytes)))
}

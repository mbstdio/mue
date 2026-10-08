use std::{
    collections::HashMap,
    fs::File,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};
use gpui::{
    App, Application, Bounds, Context, Entity, PathPromptOptions, Render, SharedString, Window,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_component::{
    ActiveTheme, Root, Theme, ThemeMode,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
};
use mue_core::{
    FFMPEG_VERSION, Request,
    conversion::{Engine, JobStatus},
    profiles::{EncodingSpeed, MediaKind, OutputFormat, Profile, Settings},
};

use crate::{ipc::Incoming, platform};

type SharedSettings = Arc<Mutex<Settings>>;

pub fn run(settings: SharedSettings, receiver: mpsc::Receiver<Incoming>, lock: File) -> Result<()> {
    let engine = Engine::new();
    let app_engine = engine.clone();
    Application::new().run(move |cx| {
        gpui_component::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        let engine = app_engine;
        let stop_engine = engine.clone();
        cx.on_app_quit(move |_| {
            // Native macOS termination need not return from Application::run. Reap encoders here.
            stop_engine.stop_and_wait();
            async {}
        })
        .detach();
        // GPUI exits after its last Windows window closes. Keep a hidden window for the tray lifetime.
        #[cfg(windows)]
        if let Err(error) = cx.open_window(
            WindowOptions {
                show: false,
                focus: false,
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1.0), px(1.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Empty),
        ) {
            platform::show_error(&format!("Cannot create background window: {error:#}"));
            cx.quit();
            return;
        }
        let tray = match platform::Tray::new() {
            Ok(tray) => tray,
            Err(error) => {
                platform::show_error(&format!("Cannot create tray icon: {error:#}"));
                cx.quit();
                return;
            }
        };
        let mut runtime = Runtime {
            settings,
            engine,
            receiver,
            tray,
            _lock: lock,
            settings_window: None,
            progress_window: None,
            idle_since: None,
            last_job: None,
        };
        runtime.poll(cx);
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                if cx.update(|cx| runtime.poll(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    });
    engine.stop_and_wait();
    Ok(())
}

#[cfg(windows)]
struct Empty;
#[cfg(windows)]
impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

struct Runtime {
    settings: SharedSettings,
    engine: Engine,
    receiver: mpsc::Receiver<Incoming>,
    tray: platform::Tray,
    _lock: File,
    settings_window: Option<WindowHandle<Root>>,
    progress_window: Option<(WindowHandle<Root>, Entity<ProgressView>)>,
    idle_since: Option<Instant>,
    last_job: Option<uuid::Uuid>,
}

impl Runtime {
    fn handle(&mut self, request: Request, cx: &mut App) -> Result<()> {
        match request {
            Request::Background => {
                #[cfg(not(windows))]
                self.open_settings(cx)?;
                Ok(())
            }
            Request::Settings => self.open_settings(cx),
            Request::Progress => self.open_progress(cx),
            Request::Quit => {
                self.engine.stop();
                cx.quit();
                Ok(())
            }
            Request::Convert { choice, files } => {
                let profile = self.settings.lock().unwrap().resolve(&choice)?;
                self.engine.enqueue(files, profile)?;
                self.idle_since = None;
                self.open_progress(cx)
            }
        }
    }

    fn poll(&mut self, cx: &mut App) {
        let incoming: Vec<_> = self.receiver.try_iter().collect();
        for incoming in incoming {
            let result = self
                .handle(incoming.request, cx)
                .map_err(|error| format!("{error:#}"));
            if let Some(reply) = incoming.reply {
                let _ = reply.send(result);
            } else if let Err(message) = &result {
                platform::show_error(message);
            }
        }
        for request in self.tray.requests() {
            if let Err(error) = self.handle(request, cx) {
                platform::show_error(&format!("{error:#}"));
            }
        }
        let (busy, failed, last_job) = {
            let state = self.engine.state.lock().unwrap();
            (
                state.jobs.iter().any(|j| j.status.active()),
                state
                    .jobs
                    .iter()
                    .any(|j| matches!(j.status, JobStatus::Failed(_))),
                state.jobs.back().map(|job| job.id),
            )
        };
        if last_job.is_some() && last_job != self.last_job {
            let _ = self.open_progress(cx);
            self.idle_since = None;
        }
        self.last_job = last_job;
        if busy {
            self.idle_since = None;
        }
        if let Some((handle, view)) = &self.progress_window {
            let view = view.clone();
            if handle
                .update(cx, |_, _, cx| view.update(cx, |_, cx| cx.notify()))
                .is_err()
            {
                self.progress_window = None;
            } else if !busy && !failed {
                let since = self.idle_since.get_or_insert_with(Instant::now);
                if since.elapsed() > Duration::from_secs(6) {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                    self.progress_window = None;
                }
            }
        }
    }

    fn open_settings(&mut self, cx: &mut App) -> Result<()> {
        if let Some(handle) = self.settings_window {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                return Ok(());
            }
        }
        let settings = self.settings.clone();
        let engine = self.engine.clone();
        let bounds = Bounds::centered(None, size(px(1000.0), px(800.0)), cx);
        self.settings_window = Some(cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(860.0), px(650.0))),
                ..Default::default()
            },
            |window, cx| {
                window.set_window_title("Mue — Settings");
                let view = cx.new(|cx| SettingsView::new(settings, engine, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )?);
        Ok(())
    }

    fn open_progress(&mut self, cx: &mut App) -> Result<()> {
        if let Some((handle, _)) = &self.progress_window {
            if handle.update(cx, |_, _, _| ()).is_ok() {
                return Ok(());
            }
        }
        let engine = self.engine.clone();
        let mut progress_view = None;
        let handle = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(platform::progress_bounds(cx))),
                show: cfg!(not(windows)),
                focus: false,
                is_resizable: false,
                is_minimizable: false,
                kind: WindowKind::PopUp,
                ..Default::default()
            },
            |window, cx| {
                window.set_window_title("Mue — Conversions");
                let view = cx.new(|_| ProgressView { engine });
                progress_view = Some(view.clone());
                cx.new(|cx| Root::new(view, window, cx))
            },
        )?;
        handle.update(cx, |_, window, cx| platform::position_progress(window, cx))?;
        self.progress_window = Some((handle, progress_view.unwrap()));
        self.idle_since = None;
        Ok(())
    }
}

struct SettingsView {
    shared: SharedSettings,
    engine: Engine,
    settings: Settings,
    selected: (bool, usize),
    draft: Profile,
    inputs: HashMap<&'static str, Entity<InputState>>,
    message: String,
}

impl SettingsView {
    fn new(
        shared: SharedSettings,
        engine: Engine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = shared.lock().unwrap().clone();
        let draft = settings.defaults[0].clone();
        let mut view = Self {
            shared,
            engine,
            settings,
            selected: (true, 0),
            draft,
            inputs: HashMap::new(),
            message: String::new(),
        };
        view.populate(window, cx);
        view
    }

    fn populate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inputs.clear();
        for (key, value) in [
            ("name", self.draft.name.clone()),
            ("quality", self.draft.quality.to_string()),
            ("compression", self.draft.png_compression.to_string()),
            (
                "width",
                self.draft
                    .max_width
                    .map_or(String::new(), |v| v.to_string()),
            ),
            (
                "height",
                self.draft
                    .max_height
                    .map_or(String::new(), |v| v.to_string()),
            ),
            ("background", self.draft.jpeg_background.clone()),
            ("crf", self.draft.video_crf.to_string()),
            (
                "fps",
                self.draft.max_fps.map_or(String::new(), |v| v.to_string()),
            ),
            ("bitrate", self.draft.audio_bitrate_kbps.to_string()),
        ] {
            self.inputs.insert(
                key,
                cx.new(|cx| InputState::new(window, cx).default_value(value)),
            );
        }
        cx.notify();
    }

    fn select(&mut self, default: bool, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = (default, index);
        self.draft = if default {
            self.settings.defaults[index].clone()
        } else {
            self.settings.profiles[index].clone()
        };
        self.message.clear();
        self.populate(window, cx);
    }

    fn change_format(&mut self, format: OutputFormat, window: &mut Window, cx: &mut Context<Self>) {
        match self.read_form(cx) {
            Ok(mut draft) => {
                draft.set_format(format);
                self.draft = draft;
                self.message.clear();
                self.populate(window, cx);
            }
            Err(error) => {
                self.message = format!("{error:#}");
                cx.notify();
            }
        }
    }

    fn read_form(&self, cx: &App) -> Result<Profile> {
        let value = |key| self.inputs[key].read(cx).value().to_string();
        let optional = |key: &'static str| -> Result<Option<u32>> {
            let text = value(key);
            if text.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(
                    text.trim()
                        .parse()
                        .with_context(|| format!("Invalid {key}"))?,
                ))
            }
        };
        let mut profile = self.draft.clone();
        profile.name = value("name").trim().into();
        profile.max_width = optional("width")?;
        profile.max_height = optional("height")?;
        if profile.format.kind() == MediaKind::Image {
            profile.quality = value("quality").parse().context("Invalid image quality")?;
            profile.png_compression = value("compression")
                .parse()
                .context("Invalid PNG compression")?;
            profile.jpeg_background = value("background")
                .trim()
                .trim_start_matches('#')
                .to_uppercase();
        } else {
            profile.video_crf = value("crf").parse().context("Invalid CRF")?;
            profile.max_fps = optional("fps")?;
            profile.audio_bitrate_kbps =
                value("bitrate").parse().context("Invalid audio bitrate")?;
        }
        profile.validate()?;
        Ok(profile)
    }

    fn persist(&mut self, settings: Settings) -> Result<()> {
        settings.save()?;
        *self.shared.lock().unwrap() = settings.clone();
        self.settings = settings;
        Ok(())
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let result = (|| -> Result<()> {
            let profile = self.read_form(cx)?;
            let mut settings = self.settings.clone();
            if self.selected.0 {
                settings.defaults[self.selected.1] = profile.clone();
            } else {
                settings.profiles[self.selected.1] = profile.clone();
            }
            self.persist(settings)?;
            self.draft = profile;
            Ok(())
        })();
        self.message = match result {
            Ok(()) => "Saved. Reopen the context menu to see changes.".into(),
            Err(error) => format!("{error:#}"),
        };
        cx.notify();
    }

    fn add(&mut self, duplicate: bool, window: &mut Window, cx: &mut Context<Self>) {
        let result = (|| -> Result<()> {
            let mut profile = if duplicate {
                self.read_form(cx)?
            } else {
                Profile::new(self.draft.format)
            };
            profile.id = uuid::Uuid::new_v4();
            if duplicate {
                profile.name.push_str(" copy");
            }
            let mut settings = self.settings.clone();
            settings.profiles.push(profile);
            self.persist(settings)?;
            self.select(false, self.settings.profiles.len() - 1, window, cx);
            Ok(())
        })();
        if let Err(error) = result {
            self.message = format!("{error:#}");
        }
        cx.notify();
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.0 {
            return;
        }
        let mut settings = self.settings.clone();
        settings.profiles.remove(self.selected.1);
        match self.persist(settings) {
            Ok(()) => self.select(true, 0, window, cx),
            Err(error) => {
                self.message = format!("{error:#}");
                cx.notify();
            }
        }
    }

    fn convert_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let profile = match self.read_form(cx) {
            Ok(profile) => profile,
            Err(error) => {
                self.message = format!("{error:#}");
                cx.notify();
                return;
            }
        };
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Convert with Mue".into()),
        });
        let engine = self.engine.clone();
        cx.spawn_in(window, async move |view, cx| {
            let result = match paths.await {
                Ok(Ok(Some(files))) => engine.enqueue(files, profile),
                Ok(Ok(None)) => return,
                Ok(Err(error)) => Err(error),
                Err(error) => Err(error.into()),
            };
            let _ = view.update(cx, |view, cx| {
                view.message = result.map_or_else(
                    |error| format!("{error:#}"),
                    |_| "Conversion added to queue.".into(),
                );
                cx.notify();
            });
        })
        .detach();
    }

    fn field(&self, label: &'static str, key: &'static str) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .flex_1()
            .child(div().text_sm().child(label))
            .child(Input::new(&self.inputs[key]))
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = div()
            .id("profiles-list")
            .w(px(260.0))
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .bg(rgb(0x171821))
            .child(div().text_2xl().child("Mue"))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x9a9caf))
                    .child("IMAGE & VIDEO CONVERTER"),
            )
            .child(div().mt_4().text_sm().child("DIRECT CONVERSION DEFAULTS"));
        for (index, profile) in self.settings.defaults.iter().enumerate() {
            sidebar =
                sidebar.child(
                    Button::new(("default", index))
                        .label(profile.format.label())
                        .when(self.selected == (true, index), |button| button.primary())
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.select(true, index, window, cx)
                        })),
                );
        }
        sidebar = sidebar.child(div().mt_4().text_sm().child("SAVED PROFILES"));
        if self.settings.profiles.is_empty() {
            sidebar = sidebar.child(
                div()
                    .text_sm()
                    .text_color(rgb(0x9a9caf))
                    .child("No profiles yet. Create one below."),
            );
        }
        for (index, profile) in self.settings.profiles.iter().enumerate() {
            sidebar = sidebar.child(
                Button::new(("profile", index))
                    .label(SharedString::from(profile.name.clone()))
                    .when(self.selected == (false, index), |button| button.primary())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.select(false, index, window, cx)
                    })),
            );
        }
        sidebar = sidebar.child(
            Button::new("new-profile")
                .label("+ New profile")
                .on_click(cx.listener(|view, _, window, cx| view.add(false, window, cx))),
        );
        let image = self.draft.format.kind() == MediaKind::Image;
        let mut form = div()
            .id("profile-form")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_4()
            .p_6()
            .child(div().text_2xl().child(if self.selected.0 {
                "Direct conversion settings"
            } else {
                "Conversion profile"
            }))
            .child(div().text_sm().text_color(rgb(0x9a9caf)).child(
                "Originals are preserved. Maximum sizes never upscale or distort the source.",
            ))
            .child(self.field("Name", "name"));
        let mut formats = div().flex().gap_2();
        for format in OutputFormat::ALL
            .into_iter()
            .filter(|format| format.kind() == self.draft.format.kind())
        {
            formats = formats.child(
                Button::new(format.extension())
                    .label(format.label())
                    .when(self.draft.format == format, |button| button.primary())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !view.selected.0 {
                            view.change_format(format, window, cx);
                        }
                    })),
            );
        }
        if self.selected.0 {
            form = form.child(div().child(format!("Output: {}", self.draft.format.label())));
        } else {
            form = form
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("image-kind")
                                .label("Image")
                                .when(image, |b| b.primary())
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.change_format(OutputFormat::Jpg, window, cx);
                                })),
                        )
                        .child(
                            Button::new("video-kind")
                                .label("Video")
                                .when(!image, |b| b.primary())
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.change_format(OutputFormat::Mp4, window, cx);
                                })),
                        ),
                )
                .child(formats);
        }
        form = form.child(
            div()
                .flex()
                .gap_4()
                .child(self.field("Maximum width (blank = original)", "width"))
                .child(self.field("Maximum height (blank = original)", "height")),
        );
        if image {
            form = form.child(if self.draft.format == OutputFormat::Png {
                self.field("PNG compression (0–9, lossless)", "compression")
                    .into_any_element()
            } else {
                self.field("Image quality (1–100)", "quality")
                    .into_any_element()
            });
            if self.draft.format == OutputFormat::Jpg {
                form = form.child(self.field("Transparency background (RGB hex)", "background"));
            }
        } else {
            form = form.child(
                div()
                    .flex()
                    .gap_4()
                    .child(self.field("CRF (lower = higher quality)", "crf"))
                    .child(self.field("Maximum FPS (blank = original)", "fps")),
            );
            let mut speeds = div().flex().gap_2();
            for (id, speed, label) in [
                ("fast", EncodingSpeed::Fast, "Fast"),
                ("balanced", EncodingSpeed::Balanced, "Balanced"),
                ("slow", EncodingSpeed::Slow, "Slow"),
            ] {
                speeds = speeds.child(
                    Button::new(id)
                        .label(label)
                        .when(self.draft.encoding_speed == speed, |b| b.primary())
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.draft.encoding_speed = speed;
                            cx.notify();
                        })),
                );
            }
            form = form
                .child(div().text_sm().child("Encoding speed"))
                .child(speeds)
                .child(self.field("Audio bitrate (16–512 kbps)", "bitrate"))
                .child(
                    Button::new("audio")
                        .label(if self.draft.keep_audio {
                            "Audio: keep"
                        } else {
                            "Audio: remove"
                        })
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.draft.keep_audio = !view.draft.keep_audio;
                            cx.notify();
                        })),
                );
        }
        form = form
            .child(
                Button::new("metadata")
                    .label(if self.draft.keep_metadata {
                        "Metadata: keep when supported"
                    } else {
                        "Metadata: remove"
                    })
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.draft.keep_metadata = !view.draft.keep_metadata;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("save")
                            .primary()
                            .label("Save changes")
                            .on_click(cx.listener(|view, _, _, cx| view.save(cx))),
                    )
                    .child(
                        Button::new("duplicate")
                            .label("Duplicate as profile")
                            .on_click(
                                cx.listener(|view, _, window, cx| view.add(true, window, cx)),
                            ),
                    )
                    .when(!self.selected.0, |row| {
                        row.child(
                            Button::new("delete")
                                .danger()
                                .label("Delete profile")
                                .on_click(
                                    cx.listener(|view, _, window, cx| view.delete(window, cx)),
                                ),
                        )
                    }),
            )
            .child(div().text_sm().child(self.message.clone()))
            .child(
                div()
                    .mt_4()
                    .border_t_1()
                    .border_color(rgb(0x353644))
                    .pt_4()
                    .text_sm()
                    .child(format!(
                        "Bundled engine: FFmpeg {FFMPEG_VERSION}. One conversion at a time."
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("convert-files")
                            .label("Convert files…")
                            .on_click(
                                cx.listener(|view, _, window, cx| view.convert_files(window, cx)),
                            ),
                    )
                    .child(Button::new("quit").label("Quit Mue").on_click(cx.listener(
                        |view, _, _, cx| {
                            view.engine.stop();
                            cx.quit();
                        },
                    ))),
            );
        div()
            .size_full()
            .flex()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(sidebar)
            .child(form)
    }
}

struct ProgressView {
    engine: Engine,
}

impl Render for ProgressView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.engine.state.lock().unwrap();
        let waiting = state
            .jobs
            .iter()
            .filter(|job| matches!(job.status, JobStatus::Queued))
            .count();
        let mut content = div()
            .id("conversion-list")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3();
        if state.jobs.is_empty() {
            content = content.child(div().text_sm().child("No conversions yet."));
        }
        let mut jobs: Vec<_> = state.jobs.iter().collect();
        jobs.sort_by_key(|job| !job.status.active());
        for job in jobs {
            let filename = job
                .source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let status = match &job.status {
                JobStatus::Queued => "Waiting".into(),
                JobStatus::Running => job.progress.map_or("Converting…".into(), |p| {
                    format!(
                        "{:.0}%{}",
                        p * 100.0,
                        job.remaining_seconds
                            .map_or(String::new(), |s| format!(" · ~{s}s remaining"))
                    )
                }),
                JobStatus::Completed(_) => "Completed".into(),
                JobStatus::Failed(error) => format!("Failed: {error}"),
                JobStatus::Cancelled => "Cancelled".into(),
            };
            let id = job.id;
            let mut row = div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_md()
                .bg(rgb(0x252631))
                .child(div().text_sm().child(filename))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x9a9caf))
                        .child(job.profile.name.clone()),
                )
                .child(div().text_xs().child(status));
            if matches!(job.status, JobStatus::Running) {
                row = row.child(
                    div()
                        .w_full()
                        .h(px(4.0))
                        .rounded_sm()
                        .bg(rgb(0x3b3c4c))
                        .child(
                            div()
                                .h_full()
                                .w(gpui::relative(job.progress.unwrap_or(0.08)))
                                .bg(rgb(0x9983ff)),
                        ),
                );
            }
            if job.status.active() {
                row = row.child(
                    Button::new(SharedString::from(format!("cancel-{id}")))
                        .label("Cancel")
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.engine.cancel(id);
                            cx.notify();
                        })),
                );
            } else if let JobStatus::Completed(output) = &job.status {
                let output = output.clone();
                row = row.child(
                    Button::new(SharedString::from(format!("reveal-{id}")))
                        .label("Show file")
                        .on_click(move |_, _, cx| cx.reveal_path(&output)),
                );
            }
            content = content.child(row);
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_lg().child(format!("Mue · {waiting} waiting")))
                    .child(
                        Button::new("dismiss")
                            .label("Hide")
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
            .child(content)
    }
}

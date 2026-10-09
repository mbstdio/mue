use crate::{ipc::Incoming, platform, preferences, progress::ProgressView, settings::SettingsView};
use anyhow::Result;
use gpui::{
    App, Application, Bounds, Context, Entity, Render, Window, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, div, prelude::*, px, size,
};
use gpui_component::Root;
use mue_core::{
    Request,
    conversion::{Engine, JobStatus},
    profiles::Settings,
};
use std::{
    fs::File,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

pub(crate) type SharedSettings = Arc<Mutex<Settings>>;

pub fn run(settings: SharedSettings, receiver: mpsc::Receiver<Incoming>, lock: File) -> Result<()> {
    let engine = Engine::new();
    let app_engine = engine.clone();
    Application::new()
        .with_assets(crate::assets::Assets)
        .run(move |cx| {
            gpui_component::init(cx);
            preferences::apply(&settings.lock().unwrap().general, cx);
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
                ui_requests: Arc::new(Mutex::new(Vec::new())),
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
    ui_requests: Arc<Mutex<Vec<Request>>>,
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
        self.tray
            .set_language(self.settings.lock().unwrap().general.language);
        let requests = std::mem::take(&mut *self.ui_requests.lock().unwrap());
        for request in requests {
            if let Err(error) = self.handle(request, cx) {
                platform::show_error(&format!("{error:#}"));
            }
        }
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
            } else if !busy && !failed && self.settings.lock().unwrap().general.auto_hide_completed
            {
                let since = self.idle_since.get_or_insert_with(Instant::now);
                if since.elapsed() > Duration::from_secs(6) {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                    self.progress_window = None;
                }
            } else {
                self.idle_since = None;
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
        let requests = self.ui_requests.clone();
        let bounds = Bounds::centered(None, size(px(1000.0), px(800.0)), cx);
        self.settings_window = Some(cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(860.0), px(650.0))),
                ..Default::default()
            },
            |window, cx| {
                window.set_window_title(&format!(
                    "Mue — {}",
                    preferences::text(settings.lock().unwrap().general.language, "Settings")
                ));
                preferences::configure_window(window, cx);
                let view = cx.new(|cx| SettingsView::new(settings, engine, requests, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )?);
        Ok(())
    }

    fn open_progress(&mut self, cx: &mut App) -> Result<()> {
        if let Some((handle, _)) = &self.progress_window {
            if handle
                .update(cx, |_, window, cx| platform::position_progress(window, cx))
                .is_ok()
            {
                return Ok(());
            }
        }
        let engine = self.engine.clone();
        let settings = self.settings.clone();
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
                window.set_window_title(&format!(
                    "Mue — {}",
                    preferences::text(settings.lock().unwrap().general.language, "Conversions")
                ));
                preferences::configure_window(window, cx);
                let view = cx.new(|_| ProgressView {
                    engine,
                    settings,
                    language: None,
                });
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

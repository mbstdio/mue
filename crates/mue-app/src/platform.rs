#[cfg(not(windows))]
use anyhow::Result;
use gpui::{App, Bounds, Pixels, Window, point, px, size};
#[cfg(not(windows))]
use mue_core::Request;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::Tray;

pub const PROGRESS_WIDTH: f32 = 480.0;
pub const PROGRESS_HEIGHT: f32 = 360.0;

#[cfg(not(windows))]
pub struct Tray;

#[cfg(not(windows))]
impl Tray {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
    pub fn requests(&self) -> Vec<Request> {
        Vec::new()
    }
}

pub fn progress_bounds(cx: &App) -> Bounds<Pixels> {
    let display = cx
        .primary_display()
        .map(|display| display.bounds())
        .unwrap_or(Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(1280.0), px(720.0)),
        ));
    Bounds::new(
        point(
            display.right() - px(PROGRESS_WIDTH + 20.0),
            display.bottom() - px(PROGRESS_HEIGHT + 40.0),
        ),
        size(px(PROGRESS_WIDTH), px(PROGRESS_HEIGHT)),
    )
}

pub fn position_progress(window: &Window, cx: &App) {
    #[cfg(windows)]
    windows::position_progress(window, cx);
    #[cfg(not(windows))]
    let _ = (window, cx);
}

pub fn show_error(message: &str) {
    #[cfg(windows)]
    windows::show_error(message);
    #[cfg(not(windows))]
    eprintln!("Mue: {message}");
}

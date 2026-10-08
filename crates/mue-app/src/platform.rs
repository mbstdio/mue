#[cfg(not(windows))]
use anyhow::Result;
use gpui::{App, Bounds, Pixels, Window, point, px, size};
#[cfg(not(windows))]
use mue_core::Request;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::Tray;

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
        point(display.right() - px(420.0), display.bottom() - px(340.0)),
        size(px(400.0), px(300.0)),
    )
}

pub fn position_progress(window: &Window) {
    #[cfg(windows)]
    windows::position_progress(window);
    #[cfg(not(windows))]
    let _ = window;
}

pub fn show_error(message: &str) {
    #[cfg(windows)]
    windows::show_error(message);
    #[cfg(not(windows))]
    eprintln!("Mue: {message}");
}

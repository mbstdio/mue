#[cfg(not(windows))]
use anyhow::Result;
use gpui::{App, Bounds, Pixels, Window, point, px, size};
#[cfg(not(windows))]
use mue_core::Request;

#[cfg(windows)]
mod startup;
#[cfg(windows)]
mod windows;

pub fn configure_startup(
    enabled: bool,
    persist: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    #[cfg(windows)]
    return startup::configure(enabled, persist);
    #[cfg(not(windows))]
    {
        anyhow::ensure!(
            !enabled,
            "Launch at sign-in is currently supported on Windows only"
        );
        persist()
    }
}

pub fn system_language_is_french() -> bool {
    #[cfg(windows)]
    {
        let mut locale = [0u16; 85];
        let length =
            unsafe { ::windows::Win32::Globalization::GetUserDefaultLocaleName(&mut locale) };
        return length > 0
            && String::from_utf16_lossy(&locale[..length as usize - 1])
                .to_ascii_lowercase()
                .starts_with("fr");
    }
    #[cfg(not(windows))]
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("fr"))
}

pub fn open_settings_folder() -> anyhow::Result<()> {
    let directory = mue_core::data_dir()?;
    std::fs::create_dir_all(&directory)?;
    let program = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(directory).spawn()?;
    Ok(())
}

#[cfg(windows)]
pub use windows::Tray;

pub const PROGRESS_WIDTH: f32 = 380.0;
pub const PROGRESS_HEIGHT: f32 = 128.0;
pub const PROGRESS_MAX_HEIGHT: f32 = 420.0;

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
    pub fn set_language(&mut self, _: mue_core::profiles::InterfaceLanguage) {}
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
            display.bottom()
                - px(if cfg!(windows) {
                    PROGRESS_HEIGHT
                } else {
                    PROGRESS_MAX_HEIGHT
                } + 40.0),
        ),
        size(px(PROGRESS_WIDTH), px(PROGRESS_HEIGHT)),
    )
}

pub fn position_progress(window: &mut Window, height: f32, follow_cursor: bool, cx: &App) {
    #[cfg(windows)]
    windows::position_progress(window, height, follow_cursor, cx);
    #[cfg(not(windows))]
    {
        window.resize(size(px(PROGRESS_WIDTH), px(height)));
        let _ = (follow_cursor, cx);
    }
}

pub fn update_titlebar(window: &Window, cx: &App) {
    #[cfg(windows)]
    windows::update_titlebar(window, cx);
    #[cfg(not(windows))]
    let _ = (window, cx);
}

pub fn show_error(message: &str) {
    #[cfg(windows)]
    windows::show_error(message);
    #[cfg(not(windows))]
    eprintln!("Mue: {message}");
}

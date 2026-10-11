use super::PROGRESS_WIDTH;
use anyhow::{Context as _, Result, ensure};
use gpui::{App, Window};
use mue_core::Request;
use mue_core::profiles::InterfaceLanguage;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

pub struct Tray {
    _icon: TrayIcon,
    settings: MenuItem,
    conversions: MenuItem,
    progress: MenuItem,
    quit: MenuItem,
    language: Option<InterfaceLanguage>,
}

impl Tray {
    pub fn new() -> Result<Self> {
        let menu = Menu::new();
        let settings = MenuItem::new("Settings", true, None);
        let conversions = MenuItem::new("Convert files", true, None);
        let progress = MenuItem::new("Conversion progress", true, None);
        let quit = MenuItem::new("Quit Mue", true, None);
        menu.append_items(&[&conversions, &settings, &progress, &quit])?;
        let mut rgba = Vec::with_capacity(32 * 32 * 4);
        for y in 0..32 {
            for x in 0..32 {
                let letter = (7..=24).contains(&y)
                    && ((7..=10).contains(&x)
                        || (21..=24).contains(&x)
                        || (y <= 18
                            && ((x as i32 - y as i32 + 1).abs() < 3
                                || (31 - x as i32 - y as i32 + 1).abs() < 3)));
                rgba.extend_from_slice(if letter {
                    &[255, 255, 255, 255]
                } else {
                    &[88, 65, 207, 255]
                });
            }
        }
        let icon = TrayIconBuilder::new()
            .with_tooltip("Mue — Image, video and audio conversion")
            .with_icon(Icon::from_rgba(rgba, 32, 32)?)
            .with_menu(Box::new(menu))
            .build()?;
        Ok(Self {
            _icon: icon,
            settings,
            conversions,
            progress,
            quit,
            language: None,
        })
    }

    pub fn requests(&self) -> Vec<Request> {
        MenuEvent::receiver()
            .try_iter()
            .filter_map(|event| {
                if event.id == *self.settings.id() {
                    Some(Request::Settings)
                } else if event.id == *self.conversions.id() {
                    Some(Request::Conversions)
                } else if event.id == *self.progress.id() {
                    Some(Request::Progress)
                } else if event.id == *self.quit.id() {
                    Some(Request::Quit)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn set_language(&mut self, language: InterfaceLanguage) {
        if self.language == Some(language) {
            return;
        }
        self.settings
            .set_text(crate::preferences::text(language, "Settings"));
        self.conversions
            .set_text(crate::preferences::text(language, "Convert files"));
        self.progress
            .set_text(crate::preferences::text(language, "Conversion progress"));
        self.quit
            .set_text(crate::preferences::text(language, "Quit Mue"));
        self.language = Some(language);
    }
}

pub fn position_progress(window: &Window, height: f32, follow_cursor: bool, cx: &App) {
    use windows::Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
            MonitorFromWindow,
        },
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            WindowsAndMessaging::{
                GetCursorPos, HWND_TOPMOST, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SetWindowPos,
                ShowWindow,
            },
        },
    };
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = HWND(handle.hwnd.get() as _);
    // Native resize/paint callbacks re-enter GPUI. Run after the root is installed and App is unborrowed.
    cx.foreground_executor()
        .spawn(async move {
            unsafe {
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                let monitor = if follow_cursor {
                    MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
                } else {
                    MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
                };
                let mut info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    return;
                }
                let (mut dpi_x, mut dpi_y) = (96, 96);
                let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
                let scale = dpi_x as f32 / 96.0;
                let width = (PROGRESS_WIDTH * scale) as i32;
                let height = (height * scale) as i32;
                let margin = (16.0 * scale) as i32;
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    info.rcWork.right - width - margin,
                    info.rcWork.bottom - height - margin,
                    width,
                    height,
                    SWP_NOACTIVATE,
                );
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        })
        .detach();
}

pub fn show_error(message: &str) {
    use windows::{
        Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
        core::HSTRING,
    };
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from("Mue"),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub(super) fn reveal_file(path: &std::path::Path) -> Result<()> {
    use std::path::{Component, PathBuf, Prefix};
    use windows::{
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::Com::{
                COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
            },
            UI::Shell::{SHOpenFolderAndSelectItems, SHParseDisplayName},
        },
        core::HSTRING,
    };

    ensure!(
        path.is_file(),
        "File is no longer available: {}",
        path.display()
    );
    // Canonical Windows paths use verbatim prefixes; Explorer expects drive or UNC paths.
    let mut shell_path = path.to_path_buf();
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        let root = match prefix.kind() {
            Prefix::VerbatimDisk(drive) => Some(PathBuf::from(format!("{}:\\", char::from(drive)))),
            Prefix::VerbatimUNC(server, share) => {
                let mut root = PathBuf::from(r"\\");
                root.push(server);
                root.push(share);
                Some(root)
            }
            _ => None,
        };
        if let Some(mut root) = root {
            root.extend(path.components().skip(2));
            shell_path = root;
        }
    }

    // Executor threads are not guaranteed to have COM initialized. Balance only our own call.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if initialized.is_err() && initialized != RPC_E_CHANGED_MODE {
        initialized
            .ok()
            .context("Cannot initialize the Windows Shell")?;
    }
    let result = (|| -> Result<()> {
        let mut item = std::ptr::null_mut();
        unsafe {
            SHParseDisplayName(
                &HSTRING::from(shell_path.as_os_str()),
                None,
                &mut item,
                0,
                None,
            )
        }
        .context("Cannot resolve file in the Windows Shell")?;
        // With no selection array, the absolute item identifies the file to select in its parent.
        let opened = unsafe { SHOpenFolderAndSelectItems(item, None, 0) };
        unsafe { CoTaskMemFree(Some(item.cast())) };
        opened.context("Cannot open the file's folder in Explorer")
    })();
    if initialized.is_ok() {
        unsafe { CoUninitialize() };
    }
    result
}

pub fn update_titlebar(window: &Window, cx: &App) {
    use gpui_component::ActiveTheme;
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute},
    };
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let dark = i32::from(cx.theme().mode.is_dark());
    unsafe {
        let _ = DwmSetWindowAttribute(
            HWND(handle.hwnd.get() as _),
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as _,
            std::mem::size_of_val(&dark) as u32,
        );
    }
}

use anyhow::Result;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;

const ICONS: &[(&str, &str)] = &[
    (
        "icons/settings.svg",
        "<path d='M4 7h16M4 17h16'/><circle cx='9' cy='7' r='3' fill='white'/><circle cx='15' cy='17' r='3' fill='white'/>",
    ),
    (
        "icons/image.svg",
        "<rect x='3' y='3' width='18' height='18' rx='2'/><circle cx='8' cy='8' r='1'/><path d='m3 17 6-6 4 4 3-3 5 5'/>",
    ),
    (
        "icons/video.svg",
        "<rect x='3' y='3' width='18' height='18' rx='2'/><path d='m10 8 6 4-6 4Z'/>",
    ),
    ("icons/check.svg", "<path d='m5 12 4 4L19 6'/>"),
    ("icons/minus.svg", "<path d='M5 12h14'/>"),
    ("icons/close.svg", "<path d='m6 6 12 12M6 18 18 6'/>"),
    ("icons/chevron-down.svg", "<path d='m6 9 6 6 6-6'/>"),
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS.iter().find(|(name, _)| *name == path).map(|(_, body)| {
            Cow::Owned(format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'>{body}</svg>").into_bytes())
        }))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}

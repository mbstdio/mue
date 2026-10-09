use std::{fs, path::Path};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ConversionChoice, data_dir};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Video,
}

impl MediaKind {
    pub fn for_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tif" | "tiff" => Some(Self::Image),
            "mp4" | "m4v" | "mov" | "mkv" | "avi" | "webm" | "mpeg" | "mpg" | "wmv" | "ts"
            | "mts" | "m2ts" => Some(Self::Video),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Jpg,
    Png,
    Webp,
    Mp4,
    Webm,
}

impl OutputFormat {
    pub const ALL: [Self; 5] = [Self::Jpg, Self::Png, Self::Webp, Self::Mp4, Self::Webm];

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Jpg => "JPG",
            Self::Png => "PNG",
            Self::Webp => "WebP",
            Self::Mp4 => "MP4 (H.264 / AAC)",
            Self::Webm => "WebM (VP9 / Opus)",
        }
    }

    pub fn kind(self) -> MediaKind {
        match self {
            Self::Jpg | Self::Png | Self::Webp => MediaKind::Image,
            Self::Mp4 | Self::Webm => MediaKind::Video,
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|format| format.extension() == value.to_ascii_lowercase())
            .context("Supported output formats: jpg, png, webp, mp4, webm")
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EncodingSpeed {
    Fast,
    Balanced,
    Slow,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub format: OutputFormat,
    pub quality: u8,
    pub png_compression: u8,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub keep_metadata: bool,
    pub jpeg_background: String,
    pub video_crf: u8,
    pub encoding_speed: EncodingSpeed,
    pub max_fps: Option<u32>,
    pub audio_bitrate_kbps: u32,
    pub keep_audio: bool,
}

impl Profile {
    pub fn new(format: OutputFormat) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: format!("{} profile", format.label()),
            format,
            quality: 85,
            png_compression: 6,
            max_width: None,
            max_height: None,
            keep_metadata: true,
            jpeg_background: "FFFFFF".into(),
            video_crf: if format == OutputFormat::Webm { 32 } else { 23 },
            encoding_speed: EncodingSpeed::Balanced,
            max_fps: None,
            audio_bitrate_kbps: 128,
            keep_audio: true,
        }
    }

    pub fn set_format(&mut self, format: OutputFormat) {
        self.format = format;
        self.video_crf = self
            .video_crf
            .min(if format == OutputFormat::Webm { 63 } else { 51 });
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 120,
            "Profile name must contain 1–120 characters"
        );
        ensure!(
            (1..=100).contains(&self.quality),
            "Image quality must be between 1 and 100"
        );
        ensure!(
            self.png_compression <= 9,
            "PNG compression must be between 0 and 9"
        );
        for dimension in [self.max_width, self.max_height].into_iter().flatten() {
            ensure!(
                (2..=32768).contains(&dimension),
                "Maximum dimensions must be between 2 and 32768"
            );
        }
        ensure!(
            self.jpeg_background.len() == 6
                && self.jpeg_background.bytes().all(|c| c.is_ascii_hexdigit()),
            "JPEG background must be a six-digit RGB hex color"
        );
        let maximum = if self.format == OutputFormat::Webm {
            63
        } else {
            51
        };
        ensure!(
            self.video_crf <= maximum,
            "Video CRF must be between 0 and {maximum}"
        );
        ensure!(
            self.max_fps.is_none_or(|fps| (1..=240).contains(&fps)),
            "Maximum frame rate must be between 1 and 240"
        );
        ensure!(
            (16..=512).contains(&self.audio_bitrate_kbps),
            "Audio bitrate must be between 16 and 512 kbps"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub schema_version: u32,
    #[serde(default)]
    pub general: GeneralSettings,
    pub defaults: Vec<Profile>,
    pub profiles: Vec<Profile>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            general: GeneralSettings::default(),
            defaults: OutputFormat::ALL.into_iter().map(Profile::new).collect(),
            profiles: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceLanguage {
    System,
    #[default]
    English,
    French,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub theme: ThemePreference,
    pub language: InterfaceLanguage,
    pub launch_at_startup: bool,
    pub auto_hide_completed: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            language: InterfaceLanguage::English,
            launch_at_startup: false,
            auto_hide_completed: true,
        }
    }
}

impl Settings {
    pub fn load() -> Result<Self> {
        let path = data_dir()?.join("profiles.json");
        match fs::read(&path) {
            Ok(bytes) => {
                let settings: Self = serde_json::from_slice(&bytes)
                    .with_context(|| format!("Invalid settings file: {}", path.display()))?;
                settings.validate()?;
                Ok(settings)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).context("Cannot read settings"),
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "Unsupported settings schema version"
        );
        for format in OutputFormat::ALL {
            ensure!(
                self.defaults.iter().filter(|p| p.format == format).count() == 1,
                "Each output format must have exactly one default profile"
            );
        }
        ensure!(
            self.defaults.len() == 5 && self.profiles.len() <= 100,
            "Too many profiles"
        );
        let mut ids = std::collections::HashSet::new();
        for profile in self.defaults.iter().chain(&self.profiles) {
            profile.validate()?;
            ensure!(ids.insert(profile.id), "Duplicate profile identifier");
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        use std::io::Write;
        self.validate()?;
        let directory = data_dir()?;
        fs::create_dir_all(&directory)?;
        let mut file = tempfile::NamedTempFile::new_in(&directory)?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.as_file().sync_all()?;
        file.persist(directory.join("profiles.json"))
            .context("Cannot save settings")?;
        Ok(())
    }

    pub fn resolve(&self, choice: &ConversionChoice) -> Result<Profile> {
        let result = match choice {
            ConversionChoice::Format(format) => self.defaults.iter().find(|p| p.format == *format),
            ConversionChoice::Profile(id) => self.profiles.iter().find(|p| p.id == *id),
        };
        match result {
            Some(profile) => {
                profile.validate()?;
                Ok(profile.clone())
            }
            None => bail!("The selected profile no longer exists"),
        }
    }
}

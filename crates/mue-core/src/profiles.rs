use std::{fs, path::Path};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::naming::FilenameTemplate;
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
    Mp4H265,
}

impl OutputFormat {
    // Append new formats to preserve the Explorer CLSIDs derived from these indices.
    pub const ALL: [Self; 6] = [
        Self::Jpg,
        Self::Png,
        Self::Webp,
        Self::Mp4,
        Self::Webm,
        Self::Mp4H265,
    ];

    pub fn command_name(self) -> &'static str {
        if self == Self::Mp4H265 {
            "mp4-h265"
        } else {
            self.extension()
        }
    }

    pub fn is_mp4(self) -> bool {
        matches!(self, Self::Mp4 | Self::Mp4H265)
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Mp4 | Self::Mp4H265 => "mp4",
            Self::Webm => "webm",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Jpg => "JPG",
            Self::Png => "PNG",
            Self::Webp => "WebP",
            Self::Mp4 => "MP4 (H.264 / AAC)",
            Self::Mp4H265 => "MP4 (H.265 / AAC)",
            Self::Webm => "WebM (VP9 / Opus)",
        }
    }

    pub fn kind(self) -> MediaKind {
        match self {
            Self::Jpg | Self::Png | Self::Webp => MediaKind::Image,
            Self::Mp4 | Self::Mp4H265 | Self::Webm => MediaKind::Video,
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|format| format.command_name() == value.to_ascii_lowercase())
            .context("Supported output formats: jpg, png, webp, mp4, mp4-h265, webm")
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EncodingSpeed {
    Fast,
    Balanced,
    Slow,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mp4Codec {
    #[default]
    H264,
    H265,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateControl {
    #[default]
    Crf,
    Cbr,
    VbrOnePass,
    VbrTwoPass,
}

fn default_video_bitrate() -> u32 {
    5000
}

fn default_filename_template() -> String {
    "{filename}.{ext}".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    #[serde(default = "default_filename_template")]
    pub filename_template: String,
    pub format: OutputFormat,
    pub quality: u8,
    pub png_compression: u8,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub keep_metadata: bool,
    pub jpeg_background: String,
    pub video_crf: u8,
    // Read the former codec field only to migrate existing MP4 profiles.
    #[serde(default, skip_serializing)]
    mp4_codec: Mp4Codec,
    #[serde(default)]
    pub rate_control: RateControl,
    #[serde(default = "default_video_bitrate")]
    pub video_bitrate_kbps: u32,
    #[serde(default)]
    pub max_video_bitrate_kbps: Option<u32>,
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
            filename_template: default_filename_template(),
            format,
            quality: 85,
            png_compression: 6,
            max_width: None,
            max_height: None,
            keep_metadata: true,
            jpeg_background: "FFFFFF".into(),
            video_crf: match format {
                OutputFormat::Webm => 32,
                OutputFormat::Mp4H265 => 28,
                _ => 23,
            },
            mp4_codec: Mp4Codec::H264,
            rate_control: RateControl::Crf,
            video_bitrate_kbps: default_video_bitrate(),
            max_video_bitrate_kbps: None,
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
        if format == OutputFormat::Webm
            && matches!(
                self.rate_control,
                RateControl::Crf | RateControl::VbrOnePass
            )
        {
            self.max_video_bitrate_kbps = None;
        }
    }

    pub fn validate(&self) -> Result<()> {
        FilenameTemplate::parse(&self.filename_template)?
            .validate(self.format.extension(), &self.name)?;
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
        ensure!(
            (1..=1_000_000).contains(&self.video_bitrate_kbps),
            "Video bitrate must be between 1 and 1000000 kbps"
        );
        if let Some(maximum) = self.max_video_bitrate_kbps {
            ensure!(
                !(self.format == OutputFormat::Mp4
                    && self.rate_control == RateControl::Crf
                    && self.video_crf == 0),
                "H.264 CRF 0 (lossless) cannot use a maximum bitrate"
            );
            ensure!(
                (1..=1_000_000).contains(&maximum),
                "Maximum video bitrate must be between 1 and 1000000 kbps"
            );
            if self.format.kind() == MediaKind::Video
                && matches!(
                    self.rate_control,
                    RateControl::VbrOnePass | RateControl::VbrTwoPass
                )
            {
                ensure!(
                    maximum >= self.video_bitrate_kbps,
                    "Maximum video bitrate must not be below the target bitrate"
                );
            }
            ensure!(
                self.format != OutputFormat::Webm
                    || matches!(
                        self.rate_control,
                        RateControl::Cbr | RateControl::VbrTwoPass
                    ),
                "VP9 maximum bitrate requires CBR or two-pass VBR"
            );
        }
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
    #[serde(default, skip_serializing)]
    pub h265_profile_initialized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            general: GeneralSettings::default(),
            defaults: OutputFormat::ALL.into_iter().map(Profile::new).collect(),
            profiles: Vec::new(),
            h265_profile_initialized: false,
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
                let mut settings: Self = serde_json::from_slice(&bytes)
                    .with_context(|| format!("Invalid settings file: {}", path.display()))?;
                let legacy_mp4_default = settings.defaults.iter().any(|profile| {
                    profile.format == OutputFormat::Mp4 && profile.mp4_codec == Mp4Codec::H265
                });
                let mut migrated = false;
                for profile in settings.defaults.iter_mut().chain(&mut settings.profiles) {
                    if profile.format == OutputFormat::Mp4 && profile.mp4_codec == Mp4Codec::H265 {
                        profile.format = OutputFormat::Mp4H265;
                        profile.mp4_codec = Mp4Codec::H264;
                        migrated = true;
                    }
                }
                if !settings
                    .defaults
                    .iter()
                    .any(|profile| profile.format == OutputFormat::Mp4H265)
                {
                    let initial = if settings.h265_profile_initialized {
                        settings.profiles.iter().position(|profile| {
                            profile.name == "MP4 H.265" && profile.format == OutputFormat::Mp4H265
                        })
                    } else {
                        None
                    };
                    let profile = initial
                        .map(|index| settings.profiles.remove(index))
                        .unwrap_or_else(|| Profile::new(OutputFormat::Mp4H265));
                    settings.defaults.push(profile);
                    migrated = true;
                }
                if legacy_mp4_default
                    && !settings
                        .defaults
                        .iter()
                        .any(|profile| profile.format == OutputFormat::Mp4)
                {
                    settings.defaults.push(Profile::new(OutputFormat::Mp4));
                    migrated = true;
                }
                settings.validate()?;
                if migrated {
                    settings.save()?;
                }
                Ok(settings)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Persist identifiers before settings and Explorer share conversion defaults.
                let settings = Self::default();
                settings.save()?;
                Ok(settings)
            }
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
            self.defaults.len() == OutputFormat::ALL.len() && self.profiles.len() <= 100,
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

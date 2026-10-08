pub mod conversion;
pub mod profiles;

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const FFMPEG_VERSION: &str = env!("MUE_FFMPEG_VERSION");

pub fn data_dir() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .context("Cannot locate the user data directory")?
        .join("Mue"))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConversionChoice {
    Format(profiles::OutputFormat),
    Profile(uuid::Uuid),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Background,
    Settings,
    Progress,
    Quit,
    Convert {
        choice: ConversionChoice,
        files: Vec<PathBuf>,
    },
}

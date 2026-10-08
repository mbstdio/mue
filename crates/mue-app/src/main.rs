#![cfg_attr(windows, windows_subsystem = "windows")]

mod ipc;
mod platform;
mod ui;

use anyhow::{Context, Result, bail, ensure};
use mue_core::{ConversionChoice, Request, data_dir, profiles::OutputFormat};
use std::{fs, path::PathBuf, sync::Arc};

fn main() {
    if let Err(error) = run() {
        platform::show_error(&format!("{error:#}"));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let request = parse_request()?;
    match ipc::start(request)? {
        ipc::Instance::Forwarded => Ok(()),
        ipc::Instance::Primary { _lock, receiver } => {
            let settings = mue_core::profiles::Settings::load()?;
            ui::run(Arc::new(std::sync::Mutex::new(settings)), receiver, _lock)
        }
    }
}

fn parse_request() -> Result<Request> {
    let mut args = std::env::args_os().skip(1);
    let Some(command) = args.next() else {
        return Ok(Request::Settings);
    };
    let request = match command.to_str().context("Invalid command")? {
        "--background" => Request::Background,
        "--progress" => Request::Progress,
        "--settings" => Request::Settings,
        "--quit" => Request::Quit,
        "--request-file" => {
            let path = PathBuf::from(args.next().context("Missing request file")?);
            let directory = data_dir()?.join("requests");
            ensure!(
                path.parent() == Some(directory.as_path()),
                "Request file must be inside the Mue requests directory"
            );
            let bytes = fs::read(&path)?;
            ensure!(bytes.len() <= 4 * 1024 * 1024, "Request file is too large");
            let request = serde_json::from_slice(&bytes)?;
            fs::remove_file(&path)?;
            request
        }
        "--convert" => {
            let format = args.next().context("Missing output format")?;
            let format = OutputFormat::parse(format.to_str().context("Invalid output format")?)?;
            let mut files = Vec::new();
            for file in args.by_ref() {
                files.push(fs::canonicalize(file).context("Cannot resolve input file")?);
            }
            ensure!(
                !files.is_empty(),
                "Usage: mue --convert <jpg|png|webp|mp4|webm> <files...>"
            );
            Request::Convert {
                choice: ConversionChoice::Format(format),
                files,
            }
        }
        _ => bail!(
            "Usage: mue [--settings | --progress | --background | --quit | --convert <format> <files...>]"
        ),
    };
    ensure!(args.next().is_none(), "Unexpected command-line arguments");
    Ok(request)
}

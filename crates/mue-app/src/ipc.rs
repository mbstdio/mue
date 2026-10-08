use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    sync::mpsc,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use mue_core::{Request, data_dir};

const MAX_MESSAGE: usize = 4 * 1024 * 1024;

pub struct Incoming {
    pub request: Request,
    pub reply: Option<mpsc::Sender<std::result::Result<(), String>>>,
}

pub enum Instance {
    Primary {
        _lock: File,
        receiver: mpsc::Receiver<Incoming>,
    },
    Forwarded,
}

pub fn start(request: Request) -> Result<Instance> {
    let directory = data_dir()?;
    fs::create_dir_all(&directory)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("instance.lock"))?;
    match FileExt::try_lock_exclusive(&lock) {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
            let mut connection = None;
            for _ in 0..50 {
                match connect() {
                    Ok(stream) => {
                        connection = Some(stream);
                        break;
                    }
                    Err(_) => thread::sleep(Duration::from_millis(100)),
                }
            }
            let mut stream =
                connection.context("Mue is running but its IPC endpoint is unavailable")?;
            write_message(&mut stream, &serde_json::to_vec(&request)?)?;
            let result: std::result::Result<(), String> =
                serde_json::from_slice(&read_message(&mut stream)?)?;
            result.map_err(anyhow::Error::msg)?;
            return Ok(Instance::Forwarded);
        }
        Err(error) => return Err(error).context("Cannot acquire application instance lock"),
    }
    let server = Server::bind()?;
    let (sender, receiver) = mpsc::channel();
    sender.send(Incoming {
        request,
        reply: None,
    })?;
    thread::spawn(move || {
        let mut server = server;
        loop {
            let result = server.accept().and_then(|mut stream| {
                let result = (|| -> Result<()> {
                    let request: Request = serde_json::from_slice(&read_message(&mut stream)?)?;
                    let (reply, response) = mpsc::channel();
                    sender.send(Incoming {
                        request,
                        reply: Some(reply),
                    })?;
                    response
                        .recv_timeout(Duration::from_secs(10))?
                        .map_err(anyhow::Error::msg)
                })();
                let response = result.map_err(|error| format!("{error:#}"));
                write_message(&mut stream, &serde_json::to_vec(&response)?)
            });
            if let Err(error) = result {
                eprintln!("IPC: {error:#}");
            }
        }
    });
    Ok(Instance::Primary {
        _lock: lock,
        receiver,
    })
}

fn write_message(stream: &mut impl Write, bytes: &[u8]) -> Result<()> {
    ensure!(bytes.len() <= MAX_MESSAGE, "IPC request is too large");
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()?;
    Ok(())
}

fn read_message(stream: &mut impl Read) -> Result<Vec<u8>> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    ensure!(length <= MAX_MESSAGE, "IPC request is too large");
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes)?;
    Ok(bytes)
}

#[cfg(windows)]
fn endpoint() -> Result<String> {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in data_dir()?.to_string_lossy().to_lowercase().bytes() {
        hash = (hash ^ byte as u64).wrapping_mul(0x100000001b3);
    }
    Ok(format!(r"\\.\pipe\Mue-{hash:x}"))
}

#[cfg(windows)]
fn connect() -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .open(endpoint()?)?)
}

#[cfg(windows)]
struct Server {
    pending: Option<File>,
}

#[cfg(windows)]
impl Server {
    fn bind() -> Result<Self> {
        Ok(Self {
            pending: Some(Self::create()?),
        })
    }

    fn create() -> Result<File> {
        use std::os::windows::io::FromRawHandle;
        use windows::{
            Win32::{
                Foundation::INVALID_HANDLE_VALUE, Storage::FileSystem::PIPE_ACCESS_DUPLEX,
                System::Pipes::*,
            },
            core::HSTRING,
        };
        let handle = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(endpoint()?),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX_MESSAGE as u32,
                MAX_MESSAGE as u32,
                5000,
                None,
            )
        };
        ensure!(
            handle != INVALID_HANDLE_VALUE,
            "Cannot create named pipe: {}",
            windows::core::Error::from_thread()
        );
        Ok(unsafe { File::from_raw_handle(handle.0) })
    }

    fn accept(&mut self) -> Result<File> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{
            Foundation::{ERROR_PIPE_CONNECTED, HANDLE},
            System::Pipes::ConnectNamedPipe,
        };
        let file = match self.pending.take() {
            Some(file) => file,
            None => Self::create()?,
        };
        if let Err(error) = unsafe { ConnectNamedPipe(HANDLE(file.as_raw_handle()), None) } {
            if error.code() != ERROR_PIPE_CONNECTED.to_hresult() {
                return Err(error.into());
            }
        }
        Ok(file)
    }
}

#[cfg(unix)]
fn connect() -> Result<std::os::unix::net::UnixStream> {
    let stream = std::os::unix::net::UnixStream::connect(data_dir()?.join("instance.sock"))?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    Ok(stream)
}

#[cfg(unix)]
struct Server(std::os::unix::net::UnixListener);

#[cfg(unix)]
impl Server {
    fn bind() -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        let path = data_dir()?.join("instance.sock");
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let server = std::os::unix::net::UnixListener::bind(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(Self(server))
    }

    fn accept(&mut self) -> Result<std::os::unix::net::UnixStream> {
        let (stream, _) = self.0.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;
        Ok(stream)
    }
}

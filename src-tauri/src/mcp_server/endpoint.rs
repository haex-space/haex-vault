//! Where the `--mcp-agent` helper reaches the app (spec 021 research R1, ADR-0005): a Unix socket in a
//! directory only this user may enter, or on Windows a named pipe this process created first. Never
//! a network interface, so neither a web page nor another device can reach it (US8).
//!
//! The name carries the first twelve hex digits of this device's `vault_device_uuid`, so two holzi
//! instances with different vaults (spec 030) get two endpoints and a socket path stays under the
//! 104 bytes macOS allows.

use std::io;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

/// The prefix of every endpoint name; the helper looks for it.
pub const ENDPOINT_PREFIX: &str = "mcp-";

/// A connection to the endpoint, from either side.
pub trait Duplex: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Duplex for T {}

pub type BoxStream = Box<dyn Duplex>;

/// `mcp-` and the first twelve hex digits of `device`.
pub fn endpoint_name(device: Uuid) -> String {
    let simple = device.simple().to_string();
    format!("{ENDPOINT_PREFIX}{}", &simple[..12])
}

/// Whether a peer may talk to the endpoint: only the user who owns it.
pub fn peer_allowed(peer_uid: u32, owner_uid: u32) -> bool {
    peer_uid == owner_uid
}

/// The directory of the sockets: the user's runtime directory where the system has one (Linux),
/// else the user's own temp directory (macOS), else the cache directory. On Windows the pipes need
/// none; the value is unused there.
pub fn default_socket_dir() -> PathBuf {
    let base = dirs::runtime_dir()
        .or_else(|| cfg!(target_os = "macos").then(std::env::temp_dir))
        .or_else(dirs::cache_dir)
        .unwrap_or_else(std::env::temp_dir);
    base.join("holzi-mcp")
}

#[cfg(unix)]
pub use unix::{bind, connect, discover, socket_path, Listener};
#[cfg(windows)]
pub use windows::{bind, connect, discover, pipe_name, Listener};

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use tokio::net::{UnixListener, UnixStream};

    use super::*;

    /// The socket of `name` in `dir`.
    pub fn socket_path(dir: &Path, name: &str) -> PathBuf {
        dir.join(format!("{name}.sock"))
    }

    /// The listening socket; removed again when dropped.
    pub struct Listener {
        inner: UnixListener,
        path: PathBuf,
        owner: u32,
    }

    /// Creates `dir` for this user only and listens on the socket of `device` in it. A socket file
    /// nobody answers on is left over from a crash and replaced; one that answers belongs to a
    /// running instance of the same vault and is kept.
    pub async fn bind(dir: &Path, device: Uuid) -> io::Result<Listener> {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        let owner = std::fs::metadata(dir)?.uid();
        let path = socket_path(dir, &endpoint_name(device));
        if path.exists() {
            if UnixStream::connect(&path).await.is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "another holzi serves this vault",
                ));
            }
            std::fs::remove_file(&path)?;
        }
        let inner = UnixListener::bind(&path)?;
        Ok(Listener { inner, path, owner })
    }

    impl Listener {
        /// The next connection; `None` for a peer of another user, which is dropped at once.
        pub async fn accept(&mut self) -> io::Result<Option<BoxStream>> {
            let (stream, _) = self.inner.accept().await?;
            let allowed = stream
                .peer_cred()
                .map(|cred| peer_allowed(cred.uid(), self.owner))
                .unwrap_or(false);
            Ok(allowed.then(|| Box::new(stream) as BoxStream))
        }

        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    /// The endpoint names in `dir`.
    pub fn discover(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|file| file.strip_suffix(".sock").map(str::to_owned))
            .filter(|name| name.starts_with(ENDPOINT_PREFIX))
            .collect();
        names.sort();
        names
    }

    pub async fn connect(dir: &Path, name: &str) -> io::Result<BoxStream> {
        Ok(Box::new(UnixStream::connect(socket_path(dir, name)).await?))
    }
}

#[cfg(windows)]
mod windows {
    use std::time::Duration;

    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeServer, PipeMode, ServerOptions,
    };

    use super::*;

    /// `ERROR_PIPE_BUSY`: every instance is taken; the client waits and tries again.
    const ERROR_PIPE_BUSY: i32 = 231;

    pub fn pipe_name(name: &str) -> String {
        format!(r"\\.\pipe\holzi-{name}")
    }

    /// The waiting pipe instance; the next one is created when a client takes this one.
    pub struct Listener {
        name: String,
        next: NamedPipeServer,
    }

    fn server(name: &str, first: bool) -> io::Result<NamedPipeServer> {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .pipe_mode(PipeMode::Byte)
            .create(name)
    }

    /// Creates the first instance; it fails when another process holds the name, so nobody can
    /// sit on the pipe before holzi (research R1). The default DACL admits only this user, the
    /// system and administrators.
    pub async fn bind(_dir: &Path, device: Uuid) -> io::Result<Listener> {
        let name = pipe_name(&endpoint_name(device));
        let next = server(&name, true).map_err(|error| {
            // ERROR_ACCESS_DENIED: another process owns the first instance of this name.
            if error.raw_os_error() == Some(5) {
                io::Error::new(io::ErrorKind::AddrInUse, "another holzi serves this vault")
            } else {
                error
            }
        })?;
        Ok(Listener { name, next })
    }

    impl Listener {
        pub async fn accept(&mut self) -> io::Result<Option<BoxStream>> {
            self.next.connect().await?;
            let connected = std::mem::replace(&mut self.next, server(&self.name, false)?);
            Ok(Some(Box::new(connected) as BoxStream))
        }
    }

    /// The endpoint names of the running instances.
    pub fn discover(_dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(r"\\.\pipe\") else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter_map(|pipe| pipe.strip_prefix("holzi-").map(str::to_owned))
            .filter(|name| name.starts_with(ENDPOINT_PREFIX))
            .collect();
        names.sort();
        names
    }

    pub async fn connect(_dir: &Path, name: &str) -> io::Result<BoxStream> {
        let pipe = pipe_name(name);
        for _ in 0..20 {
            match ClientOptions::new().open(&pipe) {
                Ok(client) => return Ok(Box::new(client)),
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "the pipe stays busy",
        ))
    }
}

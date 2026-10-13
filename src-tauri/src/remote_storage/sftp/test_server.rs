//! An SFTP server in the test process (spec 050 research R14, T063): `russh` with an SFTP subsystem
//! over a temporary folder, on a free port of the loopback interface. Its host key, its password
//! and its public key are the test's; it counts every authentication attempt (SC-004: a changed
//! host key gets none) and offers `posix-rename@openssh.com` only when asked to. A plain rename
//! refuses an existing target, as OpenSSH does.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{Auth, ChannelOpenHandle, Msg, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Packet, Status, StatusCode, Version,
};
use tokio::net::TcpListener;

/// What the server accepts and offers.
pub(crate) struct Options {
    pub host_key: PrivateKey,
    pub username: String,
    pub password: Option<String>,
    pub public_key: Option<PublicKey>,
    pub posix_rename: bool,
}

impl Options {
    /// A server with a fresh Ed25519 host key from `seed` and the login `anna`/`password`.
    pub fn new(seed: u8) -> Self {
        Self {
            host_key: PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32])),
            username: "anna".to_owned(),
            password: Some("password".to_owned()),
            public_key: None,
            posix_rename: true,
        }
    }
}

/// A running server; it ends with the test's runtime.
pub(crate) struct TestServer {
    pub address: SocketAddr,
    pub root: tempfile::TempDir,
    /// Authentication attempts so far (password or public key).
    pub attempts: Arc<AtomicUsize>,
    /// The server's host key as an OpenSSH line.
    pub host_key: String,
}

impl TestServer {
    /// `sftp://127.0.0.1:<port>`.
    pub fn endpoint(&self) -> String {
        format!("sftp://{}", self.address)
    }

    /// The path of `relative` below the server's folder.
    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }
}

/// Starts a server with `options`.
pub(crate) async fn start(options: Options) -> TestServer {
    let root = tempfile::tempdir().expect("temp dir");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let attempts = Arc::new(AtomicUsize::new(0));
    let host_key = options
        .host_key
        .public_key()
        .to_openssh()
        .expect("host key line");
    let config = Arc::new(russh::server::Config {
        keys: vec![options.host_key.clone()],
        auth_rejection_time: std::time::Duration::from_millis(1),
        auth_rejection_time_initial: Some(std::time::Duration::ZERO),
        ..Default::default()
    });
    let shared = Arc::new(Shared {
        options,
        root: root.path().to_owned(),
        attempts: Arc::clone(&attempts),
    });
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let handler = Ssh {
                shared: Arc::clone(&shared),
                channels: HashMap::new(),
            };
            let config = Arc::clone(&config);
            tokio::spawn(async move {
                if let Ok(session) = russh::server::run_stream(config, stream, handler).await {
                    let _ = session.await;
                }
            });
        }
    });
    TestServer {
        address,
        root,
        attempts,
        host_key,
    }
}

struct Shared {
    options: Options,
    root: PathBuf,
    attempts: Arc<AtomicUsize>,
}

struct Ssh {
    shared: Arc<Shared>,
    channels: HashMap<ChannelId, Channel<Msg>>,
}

impl russh::server::Handler for Ssh {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        self.shared.attempts.fetch_add(1, Ordering::SeqCst);
        let options = &self.shared.options;
        Ok(
            if user == options.username && options.password.as_deref() == Some(password) {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }

    async fn auth_publickey_offered(
        &mut self,
        user: &str,
        key: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        self.shared.attempts.fetch_add(1, Ordering::SeqCst);
        Ok(self.key_fits(user, key))
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(self.key_fits(user, key))
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        match (name, self.channels.remove(&channel_id)) {
            ("sftp", Some(channel)) => {
                session.channel_success(channel_id)?;
                let sftp = Sftp {
                    shared: Arc::clone(&self.shared),
                    handles: HashMap::new(),
                    next: 0,
                };
                russh_sftp::server::run(channel.into_stream(), sftp).await;
            }
            _ => session.channel_failure(channel_id)?,
        }
        Ok(())
    }
}

impl Ssh {
    fn key_fits(&self, user: &str, key: &PublicKey) -> Auth {
        let options = &self.shared.options;
        let fits = options
            .public_key
            .as_ref()
            .is_some_and(|own| own.key_data() == key.key_data());
        if user == options.username && fits {
            Auth::Accept
        } else {
            Auth::reject()
        }
    }
}

enum Open {
    File(fs::File),
    Dir { path: PathBuf, read: bool },
}

struct Sftp {
    shared: Arc<Shared>,
    handles: HashMap<String, Open>,
    next: u64,
}

fn status(id: u32, code: StatusCode) -> Status {
    Status {
        id,
        status_code: code,
        error_message: String::new(),
        language_tag: "en-US".to_owned(),
    }
}

fn code_of(error: std::io::Error) -> StatusCode {
    match error.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}

impl Sftp {
    fn path(&self, path: &str) -> PathBuf {
        let relative = Path::new(path.trim_start_matches('/'));
        self.shared.root.join(relative)
    }

    fn keep(&mut self, open: Open) -> String {
        self.next += 1;
        let handle = self.next.to_string();
        self.handles.insert(handle.clone(), open);
        handle
    }
}

impl russh_sftp::server::Handler for Sftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        let mut version = Version::new();
        if self.shared.options.posix_rename {
            version
                .extensions
                .insert("posix-rename@openssh.com".to_owned(), "1".to_owned());
        }
        Ok(version)
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let path = self.path(&filename);
        let file = fs::OpenOptions::new()
            .read(flags.contains(OpenFlags::READ))
            .write(flags.contains(OpenFlags::WRITE))
            .create(flags.contains(OpenFlags::CREATE))
            .truncate(flags.contains(OpenFlags::TRUNCATE))
            .open(path)
            .map_err(code_of)?;
        Ok(Handle {
            id,
            handle: self.keep(Open::File(file)),
        })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.handles.remove(&handle);
        Ok(status(id, StatusCode::Ok))
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let Some(Open::File(file)) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        file.seek(SeekFrom::Start(offset)).map_err(code_of)?;
        let mut data = vec![0; len as usize];
        let read = file.read(&mut data).map_err(code_of)?;
        if read == 0 {
            return Err(StatusCode::Eof);
        }
        data.truncate(read);
        Ok(Data { id, data })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let Some(Open::File(file)) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        file.seek(SeekFrom::Start(offset)).map_err(code_of)?;
        file.write_all(&data).map_err(code_of)?;
        Ok(status(id, StatusCode::Ok))
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        self.stat(id, path).await
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let metadata = fs::metadata(self.path(&path)).map_err(code_of)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&metadata),
        })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        let Some(Open::File(file)) = self.handles.get(&handle) else {
            return Err(StatusCode::Failure);
        };
        let metadata = file.metadata().map_err(code_of)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&metadata),
        })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let path = self.path(&path);
        if !fs::metadata(&path).map_err(code_of)?.is_dir() {
            return Err(StatusCode::NoSuchFile);
        }
        Ok(Handle {
            id,
            handle: self.keep(Open::Dir { path, read: false }),
        })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let Some(Open::Dir { path, read }) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        if *read {
            return Err(StatusCode::Eof);
        }
        *read = true;
        // Like OpenSSH, the listing holds `.` and `..`.
        let here = FileAttributes::from(&fs::metadata(&*path).map_err(code_of)?);
        let mut files = vec![File::new(".", here.clone()), File::new("..", here)];
        for entry in fs::read_dir(&*path).map_err(code_of)? {
            let entry = entry.map_err(code_of)?;
            let metadata = entry.metadata().map_err(code_of)?;
            files.push(File::new(
                entry.file_name().to_string_lossy(),
                FileAttributes::from(&metadata),
            ));
        }
        Ok(Name { id, files })
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        fs::remove_file(self.path(&filename)).map_err(code_of)?;
        Ok(status(id, StatusCode::Ok))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        fs::create_dir(self.path(&path)).map_err(code_of)?;
        Ok(status(id, StatusCode::Ok))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        fs::remove_dir(self.path(&path)).map_err(code_of)?;
        Ok(status(id, StatusCode::Ok))
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let path = if path.is_empty() || path == "." {
            "/".to_owned()
        } else {
            format!("/{}", path.trim_start_matches('/'))
        };
        Ok(Name {
            id,
            files: vec![File::dummy(path)],
        })
    }

    /// As OpenSSH: an existing target is refused.
    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        let to = self.path(&newpath);
        if to.exists() {
            return Err(StatusCode::Failure);
        }
        fs::rename(self.path(&oldpath), to).map_err(code_of)?;
        Ok(status(id, StatusCode::Ok))
    }

    async fn extended(
        &mut self,
        id: u32,
        request: String,
        data: Vec<u8>,
    ) -> Result<Packet, Self::Error> {
        if request != "posix-rename@openssh.com" || !self.shared.options.posix_rename {
            return Err(StatusCode::OpUnsupported);
        }
        let (from, rest) = ssh_string(&data).ok_or(StatusCode::BadMessage)?;
        let (to, _) = ssh_string(rest).ok_or(StatusCode::BadMessage)?;
        fs::rename(self.path(&from), self.path(&to)).map_err(code_of)?;
        Ok(Packet::Status(status(id, StatusCode::Ok)))
    }
}

/// One SSH string (length, then bytes) and what follows it.
fn ssh_string(data: &[u8]) -> Option<(String, &[u8])> {
    let len = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as usize;
    let text = data.get(4..4 + len)?;
    Some((String::from_utf8(text.to_vec()).ok()?, &data[4 + len..]))
}

/// The count of authentication attempts on `server` so far.
pub(crate) fn attempts(server: &TestServer) -> usize {
    server.attempts.load(Ordering::SeqCst)
}

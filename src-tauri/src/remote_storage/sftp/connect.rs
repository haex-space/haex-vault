//! One SSH connection to the server of an SFTP storage (spec 050 research R3, R5, R6): holzi
//! resolves the host and checks its addresses as for every storage, opens TCP to one checked
//! address, and compares the host key with the confirmed one before any login, so a server that
//! shows another key never gets credentials (FR-012, SC-004). Then it signs in with a password or
//! a key and opens two SFTP channels: one for files, one for the extensions russh-sftp does not
//! speak (`posix-rename@openssh.com`, `statvfs@openssh.com`). Everything runs within the call's
//! deadline; the log gets kinds of failures, never an address, a login or a key.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, AuthResult, Handle};
use russh::keys::{PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use russh_sftp::client::{RawSftpSession, SftpSession};
use tokio::net::TcpStream;
use tokio::time::Instant;

use super::super::address::{self, Resolver};
use super::super::{Access, Credentials, StorageError};
use super::keys;

/// SSH sessions without traffic for this long close (the pool closes idle ones earlier).
const INACTIVITY: Duration = Duration::from_secs(120);

/// How often holzi asks a quiet server whether it is still there.
const KEEPALIVE: Duration = Duration::from_secs(30);

/// How long one SFTP request may take before russh-sftp gives up; the call's deadline is the
/// shorter limit in practice.
const REQUEST_SECONDS: u64 = 60;

/// What the server offers beyond plain SFTP, read when the session starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Extensions {
    pub posix_rename: bool,
    pub statvfs: bool,
}

/// A signed-in SSH connection with its two SFTP channels.
pub(super) struct Session {
    handle: Handle<Client>,
    pub sftp: SftpSession,
    pub raw: RawSftpSession,
    pub extensions: Extensions,
}

impl Session {
    /// Whether the connection is gone (the server closed it, or it timed out).
    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }
}

/// Why a connection failed, with the host key the server showed when that was the reason.
#[derive(Debug)]
pub(super) struct Refused {
    pub error: StorageError,
    /// The OpenSSH line of the key the server showed, when it was not the confirmed one.
    pub shown_key: Option<String>,
}

impl From<StorageError> for Refused {
    fn from(error: StorageError) -> Self {
        Self {
            error,
            shown_key: None,
        }
    }
}

/// The SSH side of holzi: it accepts exactly the confirmed host key and remembers the one shown.
struct Client {
    confirmed: Option<PublicKey>,
    shown: Arc<Mutex<Option<PublicKey>>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        shown: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = shown else {
            // A host certificate needs a trusted authority holzi does not keep.
            return Ok(false);
        };
        *self.shown.lock().expect("shown key") = Some(key.clone());
        Ok(self
            .confirmed
            .as_ref()
            .is_some_and(|confirmed| confirmed.key_data() == key.key_data()))
    }
}

/// Opens a connection for `access` before `deadline`.
pub(super) async fn connect(
    resolver: &dyn Resolver,
    access: &Access,
    deadline: Instant,
) -> Result<Session, Refused> {
    match tokio::time::timeout_at(deadline, open(resolver, access)).await {
        Ok(result) => result,
        Err(_) => Err(StorageError::TimedOut.into()),
    }
}

async fn open(resolver: &dyn Resolver, access: &Access) -> Result<Session, Refused> {
    let location = &access.location;
    let url = address::check_endpoint(&location.endpoint).map_err(|_| StorageError::Network)?;
    let pinned = address::pin(&url, location.endpoint_scope, resolver)
        .await
        .map_err(|error| {
            log::warn!("remote storage: SFTP endpoint refused: {error}");
            StorageError::Network
        })?;
    let target = *pinned.addrs.first().ok_or(StorageError::Network)?;
    let stream = TcpStream::connect(target).await.map_err(|error| {
        log::warn!("remote storage: SFTP connect failed: {}", error.kind());
        StorageError::Network
    })?;
    let _ = stream.set_nodelay(true);
    let confirmed = location.host_key.as_deref().and_then(keys::parse_host_key);
    let had_key = confirmed.is_some();
    let shown = Arc::new(Mutex::new(None));
    let client = Client {
        confirmed,
        shown: Arc::clone(&shown),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(INACTIVITY),
        keepalive_interval: Some(KEEPALIVE),
        nodelay: true,
        ..Default::default()
    });
    let mut handle = match client::connect_stream(config, stream, client).await {
        Ok(handle) => handle,
        Err(russh::Error::UnknownKey) => {
            let shown_key = shown
                .lock()
                .expect("shown key")
                .as_ref()
                .map(keys::host_key_line);
            let error = if had_key {
                log::warn!("remote storage: the SFTP server showed another host key");
                StorageError::HostKeyChanged
            } else {
                StorageError::HostKeyUnknown
            };
            return Err(Refused { error, shown_key });
        }
        Err(error) => {
            log::warn!("remote storage: SSH handshake failed: {}", kind_of(&error));
            return Err(StorageError::Network.into());
        }
    };
    sign_in(&mut handle, &access.credentials).await?;
    let sftp = SftpSession::new(subsystem(&handle).await?)
        .await
        .map_err(sftp_error)?;
    sftp.set_timeout(REQUEST_SECONDS);
    let raw = RawSftpSession::new(subsystem(&handle).await?);
    raw.set_timeout(REQUEST_SECONDS);
    let version = raw.init().await.map_err(sftp_error)?;
    let offers = |name: &str| version.extensions.contains_key(name);
    let extensions = Extensions {
        posix_rename: offers("posix-rename@openssh.com"),
        statvfs: offers("statvfs@openssh.com"),
    };
    Ok(Session {
        handle,
        sftp,
        raw,
        extensions,
    })
}

/// Signs in with the password or the key of `credentials`.
async fn sign_in(handle: &mut Handle<Client>, credentials: &Credentials) -> Result<(), Refused> {
    let result = match credentials {
        Credentials::Password(login) => handle
            .authenticate_password(login.username.clone(), login.password.as_str())
            .await
            .map_err(|error| {
                log::warn!(
                    "remote storage: SSH password login failed: {}",
                    kind_of(&error)
                );
                StorageError::Network
            })?,
        Credentials::SshKey(login) => {
            let key = keys::decode(
                &login.private_key,
                login.passphrase.as_deref().map(|p| p.as_str()),
            )
            .ok_or_else(|| {
                log::warn!("remote storage: the SSH key of a connection cannot be read");
                StorageError::AccessDenied
            })?;
            let hash = if key.algorithm().is_rsa() {
                handle
                    .best_supported_rsa_hash()
                    .await
                    .ok()
                    .flatten()
                    .flatten()
            } else {
                None
            };
            let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
            handle
                .authenticate_publickey(login.username.clone(), key)
                .await
                .map_err(|error| {
                    log::warn!("remote storage: SSH key login failed: {}", kind_of(&error));
                    StorageError::Network
                })?
        }
        Credentials::S3(_) => {
            log::warn!("remote storage: SFTP got credentials of another kind");
            return Err(StorageError::AccessDenied.into());
        }
    };
    match result {
        AuthResult::Success => Ok(()),
        AuthResult::Failure { .. } => Err(StorageError::AccessDenied.into()),
    }
}

/// A new channel with the SFTP subsystem.
async fn subsystem(handle: &Handle<Client>) -> Result<russh::ChannelStream<client::Msg>, Refused> {
    let channel = handle.channel_open_session().await.map_err(|error| {
        log::warn!("remote storage: SSH channel refused: {}", kind_of(&error));
        StorageError::Network
    })?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|error| {
            log::warn!(
                "remote storage: SFTP subsystem refused: {}",
                kind_of(&error)
            );
            StorageError::Protocol
        })?;
    Ok(channel.into_stream())
}

/// The kind of an SSH failure for the log; never its text, which may name the server.
fn kind_of(error: &russh::Error) -> &'static str {
    match error {
        russh::Error::Disconnect => "disconnected",
        russh::Error::ConnectionTimeout | russh::Error::InactivityTimeout => "timed out",
        russh::Error::NoCommonAlgo { .. } => "no common algorithm",
        russh::Error::IO(_) => "i/o",
        _ => "protocol",
    }
}

/// The error of an SFTP failure (research R10).
pub(super) fn sftp_error(error: russh_sftp::client::error::Error) -> StorageError {
    use russh_sftp::client::error::Error;
    use russh_sftp::protocol::StatusCode;
    match error {
        Error::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => StorageError::NotFound,
            StatusCode::PermissionDenied => StorageError::MissingRight,
            StatusCode::OpUnsupported => StorageError::Unsupported,
            code => {
                log::warn!("remote storage: SFTP answered {code:?}");
                StorageError::Network
            }
        },
        Error::Timeout => StorageError::TimedOut,
        Error::Limited(_) => StorageError::TooLarge,
        Error::IO(_) | Error::UnexpectedPacket | Error::UnexpectedBehavior(_) => {
            log::warn!("remote storage: SFTP channel failed");
            StorageError::Network
        }
    }
}

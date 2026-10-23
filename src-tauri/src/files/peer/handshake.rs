//! The first stream of a `haex-vault-files/1` connection (spec 056 research R4): the exchange of
//! [`foreign_auth`] (spec 057), then the serving device's [`Welcome`] with the mode, or the end of
//! the connection without a reason.

use iroh::endpoint::{Connection, RecvStream, SendStream};

use crate::contacts::service::{Caller as ContactsCaller, ContactsService, VaultCard};
use crate::error::Result;
use crate::files::peer::wire::{read_head, write_head, Mode, Welcome, VERSION};
use crate::files::share::{device_access, store, FEATURE};
use crate::sync::foreign_auth::{self, ForeignAuthError, Local, Purpose, Trust, Verified};
use crate::sync::signing::Domain;
use crate::sync::wire::FILES_ALPN;
use crate::vault_gate::VaultDb;

/// What the proofs of `haex-vault-files/1` are bound to.
pub const PURPOSE: Purpose = Purpose {
    alpn: FILES_ALPN,
    domain: Domain::FilesAuth,
};

/// The mode for a verified device (contracts/files-protocol.md): an own device by the switch, a
/// contact while one of its grants is on a share `device` serves, anything else nothing.
pub async fn mode_of(db: &VaultDb, device: &str, verified: &Verified) -> Result<Option<Mode>> {
    match &verified.trust {
        Trust::OwnVault => {
            let on = db.read(|q| Ok(device_access::enabled(q)?)).await?;
            Ok(Some(if on { Mode::OwnDevice } else { Mode::AccessOff }))
        }
        Trust::Contact { .. } => {
            let (device, vault) = (device.to_owned(), crate::sync::keys::hex(&verified.vault));
            let shares = db
                .read(move |q| Ok(store::shares_for_vault(q, &device, &vault)?))
                .await?;
            Ok((!shares.is_empty()).then_some(Mode::Contact))
        }
        Trust::Unverified => Ok(None),
    }
}

/// Whether `vault` is still a contact with a stored card and not blocked (FR-023), asked again on
/// every request of a contact.
pub async fn still_contact(db: &VaultDb, vault: [u8; 32]) -> bool {
    let caller = ContactsCaller::Internal { feature: FEATURE };
    matches!(
        ContactsService::new(db.clone())
            .card_of_vault(&caller, vault)
            .await,
        Ok(VaultCard::Stored { .. })
    )
}

/// The serving side on the first stream: the verified device and its mode, or `None` after a
/// refusal; the whole exchange within [`foreign_auth::HANDSHAKE_LIMIT`]. After a refusal the stream
/// is finished, so the caller lets the other side read it before closing.
pub async fn accept(
    local: &Local<'_>,
    db: &VaultDb,
    device: &str,
    trash: bool,
    connection: &Connection,
) -> Option<(Verified, Mode)> {
    let remote = *connection.remote_id().as_bytes();
    let exchange = async {
        let (mut send, mut recv) = connection.accept_bi().await.ok()?;
        let verified =
            match foreign_auth::accept(&mut send, &mut recv, local, PURPOSE, remote).await {
                Ok(verified) => verified,
                Err(error) => {
                    log::debug!("files: an incoming device was refused: {error}");
                    let _ = send.finish();
                    return None;
                }
            };
        let mode = match mode_of(db, device, &verified).await {
            Ok(Some(mode)) => Some(mode),
            Ok(None) => None,
            Err(error) => {
                log::warn!("files: the mode of an incoming device could not be read: {error}");
                None
            }
        };
        let Some(mode) = mode else {
            let _ = send.finish();
            return None;
        };
        let welcome = Welcome {
            v: VERSION,
            mode,
            trash,
        };
        write_head(&mut send, &welcome).await.ok()?;
        send.finish().ok()?;
        Some((verified, mode))
    };
    tokio::time::timeout(foreign_auth::HANDSHAKE_LIMIT, exchange)
        .await
        .ok()
        .flatten()
}

/// Why the dialing side could not open a connection.
#[derive(Debug, thiserror::Error)]
pub enum DialError {
    #[error("the serving device could not be reached")]
    Unreachable,
    #[error(transparent)]
    Auth(#[from] ForeignAuthError),
    #[error("the serving device refused or ended the connection")]
    Refused,
    #[error("the serving device answers another version")]
    Version,
}

/// The dialing side on the first stream of `connection`: the verified serving device and its
/// welcome.
pub async fn dial(
    local: &Local<'_>,
    connection: &Connection,
) -> std::result::Result<(Verified, Welcome), DialError> {
    let remote = *connection.remote_id().as_bytes();
    let exchange = async {
        let (mut send, mut recv): (SendStream, RecvStream) =
            connection.open_bi().await.map_err(|_| DialError::Refused)?;
        let verified = foreign_auth::dial(&mut send, &mut recv, local, PURPOSE, remote).await?;
        let welcome: Welcome = read_head(&mut recv).await.map_err(|_| DialError::Refused)?;
        if welcome.v != VERSION {
            return Err(DialError::Version);
        }
        Ok((verified, welcome))
    };
    tokio::time::timeout(foreign_auth::HANDSHAKE_LIMIT, exchange)
        .await
        .map_err(|_| DialError::Auth(ForeignAuthError::TimedOut))?
}

#[cfg(test)]
#[path = "handshake_tests.rs"]
mod tests;

//! Channels between the same extension on devices of different vaults (spec 057).
//!
//! A channel is one QUIC connection on `haex-vault-ext-peer/1` of the session endpoint: after the other
//! device proved itself ([`crate::sync::foreign_auth`]) and was admitted ([`admission`]), its
//! datagrams carry the extension's fast messages and the first stream its reliable ones
//! ([`channel`]). Everything lives in memory ([`PeerState`]) and reaches only the frames of the
//! extension the channel belongs to; nothing is stored but the permission `peers` (FR-042).

pub mod admission;
pub mod channel;
pub mod join;
pub mod lobby;
pub mod state;
pub mod throttle;
pub mod watch;
pub mod wire;

#[cfg(test)]
pub(crate) mod test_support;

use haex_crdt::rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::contacts::service::{Caller, ContactsService};
use crate::extensions::contacts::grants_of;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::ExtensionHost;
use crate::extensions::ids::{ExtensionName, PublicKey};
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::PermissionKind;
use crate::storage::query::Query;

pub use state::PeerState;

/// The holzi function channels read the address book as (`Caller::Internal`).
pub const FEATURE: &str = "extensionPeers";

/// The limits of research R14.
pub mod limits {
    use std::ops::RangeInclusive;
    use std::time::Duration;

    /// The payload of an invitation or a join, JSON in UTF-8.
    pub const PAYLOAD_BYTES: usize = 4 * 1024;
    /// The display name of a join, in characters.
    pub const DISPLAY_NAME_CHARS: RangeInclusive<usize> = 1..=64;
    /// A fast message (one datagram).
    pub const FAST_BYTES: usize = 1024;
    /// A reliable message.
    pub const RELIABLE_BYTES: usize = 64 * 1024;
    /// Open channels per extension and device.
    pub const CHANNELS_PER_EXTENSION: usize = 16;
    /// Open lobbies per extension and device.
    pub const LOBBIES_PER_EXTENSION: usize = 4;
    /// Open outgoing invitations per extension.
    pub const OUTGOING_INVITES_PER_EXTENSION: usize = 16;
    /// How long an invitation stays open.
    pub const INVITE_LIFETIME: Duration = Duration::from_secs(5 * 60);
    /// How long a device has after the handshake to ask for admission (FR-032).
    pub const ADMISSION_LIMIT: Duration = Duration::from_secs(5);
    /// How long `extension_peers_join` waits for the host's answer.
    pub const JOIN_LIMIT: Duration = Duration::from_secs(15);
}

/// An extension as both sides name it on the wire: the publisher key and the name of its manifest,
/// which together identify it (spec 017).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionRef {
    /// 64 lowercase hex characters.
    pub public_key: String,
    pub name: String,
}

impl ExtensionRef {
    /// The id this extension has on `device`: installed and enabled, or a development version of
    /// the same key and name while developer mode is on there (as `dispatch::is_enabled` decides).
    /// `None` for anything else, a key or name that is not well-formed included.
    pub fn resolve(&self, q: &mut impl Query, device: Uuid) -> crate::error::Result<Option<Uuid>> {
        let (Ok(key), Ok(name)) = (
            PublicKey::parse(&self.public_key),
            ExtensionName::parse(&self.name),
        ) else {
            return Ok(None);
        };
        let installed: Option<String> = q.query_row(
            "SELECT id FROM extensions WHERE public_key = ?1 AND name = ?2 \
             AND state = 'installed' AND enabled = 1",
            params![key.as_str(), name.as_str()],
            |r| r.get(0),
        )?;
        if let Some(id) = installed.and_then(|id| Uuid::parse_str(&id).ok()) {
            return Ok(Some(id));
        }
        if !crate::extensions::dev::mode(q, device)? {
            return Ok(None);
        }
        Ok(crate::extensions::dev::registrations(q, device)?
            .into_iter()
            .find(|r| r.prefix.public_key == key && r.prefix.name == name)
            .map(|r| r.id))
    }
}

/// Why a call about channels failed. Only the reason reaches the extension, never an address, a
/// key or anything of another extension (FR-043).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PeerError {
    #[error("contact_unknown")]
    ContactUnknown,
    #[error("invite_not_open")]
    InviteNotOpen,
    #[error("channel_unknown")]
    ChannelUnknown,
    #[error("lobby_closed")]
    LobbyClosed,
    #[error("not_admitted")]
    NotAdmitted,
    #[error("too_large")]
    TooLarge { limit: usize },
    #[error("limit")]
    Limit { limit: usize },
    #[error("channel_closed")]
    ChannelClosed,
    #[error("network_unavailable")]
    NetworkUnavailable,
    #[error("invalid_input")]
    InvalidInput,
}

impl PeerError {
    /// The fixed code of the bridge (research R13).
    fn code(self) -> ExtensionErrorCode {
        match self {
            Self::ContactUnknown
            | Self::InviteNotOpen
            | Self::ChannelUnknown
            | Self::LobbyClosed => ExtensionErrorCode::NotFound,
            Self::NotAdmitted => ExtensionErrorCode::PermissionDenied,
            Self::TooLarge { .. } | Self::Limit { .. } => ExtensionErrorCode::LimitExceeded,
            Self::ChannelClosed | Self::NetworkUnavailable => ExtensionErrorCode::NotAvailable,
            Self::InvalidInput => ExtensionErrorCode::Validation,
        }
    }
}

impl From<PeerError> for BridgeError {
    fn from(error: PeerError) -> Self {
        let reason = error.to_string();
        let details = match error {
            PeerError::TooLarge { limit } | PeerError::Limit { limit } => {
                json!({ "reason": reason, "limit": limit })
            }
            _ => json!({ "reason": reason }),
        };
        BridgeError::new(error.code(), reason).with_details(details)
    }
}

/// Whether the `contacts` permission of `extension_id` covers `contact_id` on this device: only
/// then may an event about a channel name the contact (research R12, 051 FR-031). A failed read
/// names nobody.
pub(crate) async fn contact_in_scope(
    ctx: &admission::PeerContext,
    host: &ExtensionHost,
    extension_id: Uuid,
    contact_id: &str,
) -> bool {
    let device = ctx.db.device_id();
    let Ok(mut permissions) = ctx
        .db
        .read(move |q| {
            candidates(q, extension_id, PermissionKind::Contacts, device).map_err(Into::into)
        })
        .await
    else {
        return false;
    };
    permissions.extend(
        host.permissions
            .temporary(extension_id, PermissionKind::Contacts),
    );
    let service = ContactsService::new(ctx.db.clone());
    let Ok(groups) = service.groups(&Caller::Internal { feature: FEATURE }).await else {
        return false;
    };
    let grants = grants_of(&permissions, &groups);
    let caller = Caller::Extension {
        id: extension_id.to_string(),
    };
    service.get(&caller, &grants, contact_id).await.is_ok()
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "isolation_tests.rs"]
mod isolation_tests;

#[cfg(test)]
#[path = "load_tests.rs"]
mod load_tests;

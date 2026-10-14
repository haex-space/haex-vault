//! Credentials of a connection from the window (spec 052 research R1): typed ones become a new
//! entry owned by `storage` after the passed test, a chosen entry of the password manager is read
//! for the test and then linked as it is, without a copy of its values. A saved connection shows the
//! window which entry it uses (FR-007), never a value.

use reqwest::Url;

use super::credentials_of;
use super::{invalid, StorageService};
use crate::error::Result;
use crate::remote_storage::address;
use crate::remote_storage::model::{
    ConfirmedHostKey, ConnectionView, CredentialsEntry, CredentialsInput,
};
use crate::remote_storage::sftp::keys;
use crate::remote_storage::{credentials, ConnectionRow, Credentials, CredentialsState, Protocol};

/// Builds the public connection view with credential availability and the HTTP warning flag.
pub(super) fn view(
    row: &ConnectionRow,
    credentials: CredentialsState,
    credentials_entry: Option<CredentialsEntry>,
) -> ConnectionView {
    ConnectionView {
        id: row.id.clone(),
        protocol: row.protocol,
        provider_name: row.provider_name.clone(),
        provider_kind: row.provider_kind,
        endpoint: row.endpoint.clone(),
        region: row.region.clone(),
        addressing: row.addressing,
        insecure: Url::parse(&row.endpoint).is_ok_and(|url| address::is_insecure(&url)),
        endpoint_scope: row.endpoint_scope,
        credentials,
        credentials_entry,
        host_key: row.host_key.as_deref().and_then(|line| {
            Some(ConfirmedHostKey {
                fingerprint: keys::fingerprint(line)?,
                confirmed_at: row.host_key_confirmed_at.clone().unwrap_or_default(),
            })
        }),
    }
}

/// Credentials of a save, resolved before its test.
pub(super) enum Resolved {
    /// Typed in the window; a new entry holds them once the test passed.
    Typed(Credentials),
    /// The values of the chosen entry `item_id`.
    Linked {
        item_id: String,
        credentials: Credentials,
    },
}

impl Resolved {
    /// The credentials to test with.
    pub(super) fn credentials(&self) -> &Credentials {
        match self {
            Self::Typed(credentials) | Self::Linked { credentials, .. } => credentials,
        }
    }

    /// Whether linking makes a new entry, which a failed save must take back.
    pub(super) fn creates(&self) -> bool {
        matches!(self, Self::Typed(_))
    }
}

impl StorageService {
    /// Resolves the credentials of a save for a connection of `protocol`. A chosen entry that is not
    /// there for holzi is [`crate::error::HolziError::StorageCredentialsUnavailable`]; one without the
    /// values of the protocol is invalid credentials (contracts/tauri-commands.md).
    pub(super) async fn resolve(
        &self,
        input: CredentialsInput,
        protocol: Protocol,
    ) -> Result<Resolved> {
        let CredentialsInput::Entry { item_id } = input else {
            return Ok(Resolved::Typed(credentials_of(input, protocol)?));
        };
        let item = credentials::read_item(&self.passwords, &self.db, &item_id, protocol).await?;
        let credentials =
            credentials::from_item(&item, protocol).ok_or_else(|| invalid("credentials"))?;
        Ok(Resolved::Linked {
            item_id,
            credentials,
        })
    }

    /// The entry the connection `row` points at with `resolved`: a new one for typed credentials,
    /// the chosen one as it is.
    pub(super) async fn link(&self, row: &ConnectionRow, resolved: &Resolved) -> Result<String> {
        match resolved {
            Resolved::Linked { item_id, .. } => Ok(item_id.clone()),
            Resolved::Typed(typed) => {
                credentials::create(
                    &self.passwords,
                    row.protocol,
                    &row.provider_name,
                    &row.endpoint,
                    typed,
                )
                .await
            }
        }
    }

    /// Marks a chosen entry with the tag of cloud storage credentials once its connection is saved
    /// (spec 052 FR-006, research R4), as the user who chose it. The tag is a convenience: a failure
    /// is logged and the saved connection stands.
    pub(super) async fn tag_chosen(&self, resolved: &Resolved) {
        if let Resolved::Linked { item_id, .. } = resolved {
            if let Err(error) = credentials::tag_selected(&self.passwords, item_id).await {
                log::warn!("remote storage: the chosen credentials entry was not tagged: {error}");
            }
        }
    }

    /// The connection `row` as the window sees it, with the state of its entry and the entry's title.
    pub(super) async fn view_of(&self, row: &ConnectionRow) -> Result<ConnectionView> {
        let (item_id, protocol) = (row.credentials_item_id.clone(), row.protocol);
        let (state, entry) = self
            .db
            .read(move |q| {
                Ok((
                    credentials::state_in(q, &item_id, protocol)?,
                    credentials::entry(q, &item_id)?,
                ))
            })
            .await?;
        Ok(view(row, state, entry))
    }
}

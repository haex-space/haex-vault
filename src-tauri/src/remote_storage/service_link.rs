//! Credentials of a connection from the window (spec 052 research R1): typed ones become a new
//! entry owned by `storage` after the passed test, a chosen entry of the password manager is read
//! for the test and then linked as it is, without a copy of its values.

use super::credentials_of;
use super::{invalid, StorageService};
use crate::error::Result;
use crate::remote_storage::model::CredentialsInput;
use crate::remote_storage::{credentials, ConnectionRow, Credentials, Protocol};

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
            Resolved::Linked { item_id, .. } => {
                credentials::tag_selected(&self.passwords, item_id).await?;
                Ok(item_id.clone())
            }
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
}

//! Stored Visitenkarten and blocked vaults in the service (spec 051 FR-013, FR-039): what the address
//! book knows of a vault for holzi's own handshakes, and blocking. Reading cards in is
//! `service/share.rs` (spec 060).

use super::{Caller, ContactsService};
use crate::contacts::access::require_user;
use crate::contacts::card::format::SignedCard;
use crate::contacts::card::received::{self, BlockedVault};
use crate::error::{HolziError, Result};

/// What the address book knows of a vault, for holzi's own handshakes with its devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultCard {
    /// Its card is stored on this contact, and the vault is not blocked.
    Stored {
        contact_id: String,
        card: SignedCard,
    },
    /// The person blocked the vault (FR-039), whether its card is stored or not.
    Blocked,
    /// Neither.
    Unknown,
}

/// The holzi functions that look vaults up by their identity: channels between extensions (spec
/// 057) and file shares (spec 056).
const CARD_LOOKUP_FEATURES: [&str; 2] = ["extensionPeers", "fileShares"];

fn invalid(reason: &str) -> HolziError {
    HolziError::ContactsInvalid {
        reason: reason.to_owned(),
    }
}

impl ContactsService {
    /// The stored card of `vault`, for a handshake with one of its devices. Only the holzi
    /// functions of [`CARD_LOOKUP_FEATURES`] ask; nothing of it reaches an extension.
    pub async fn card_of_vault(&self, caller: &Caller, vault: [u8; 32]) -> Result<VaultCard> {
        match caller {
            Caller::Internal { feature } if CARD_LOOKUP_FEATURES.contains(feature) => {}
            _ => return Err(HolziError::ContactsForbidden),
        }
        self.db()
            .read(move |q| {
                if received::is_blocked(q, &vault)? {
                    return Ok(VaultCard::Blocked);
                }
                Ok(match received::stored(q, &vault)? {
                    Some((contact_id, card)) => VaultCard::Stored { contact_id, card },
                    None => VaultCard::Unknown,
                })
            })
            .await
    }

    pub async fn blocked(&self, caller: &Caller) -> Result<Vec<BlockedVault>> {
        require_user(caller)?;
        self.db()
            .read(|q| received::blocked(q).map_err(Into::into))
            .await
    }

    /// Blocks or unblocks a vault (FR-039); `vault_pubkey` is 64 hex characters.
    pub async fn set_blocked(
        &self,
        caller: &Caller,
        vault_pubkey: String,
        blocked: bool,
    ) -> Result<()> {
        require_user(caller)?;
        let key = vault_pubkey.trim().to_ascii_lowercase();
        if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid("not a vault identity"));
        }
        self.db()
            .write(move |tx| {
                if blocked {
                    received::block(tx, &key)?;
                } else {
                    received::unblock(tx, &key)?;
                }
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "cards_tests.rs"]
mod tests;

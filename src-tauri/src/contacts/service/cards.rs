//! Stored Visitenkarten and blocked vaults in the service (spec 051 FR-013, FR-039): what the address
//! book knows of a vault for haex-vault's own handshakes, and blocking. Reading cards in is
//! `service/share.rs` (spec 060).

use haex_crdt::CrdtTransaction;

use super::{Caller, ContactsService};
use crate::chat::peer::delivery::now_ms;
use crate::chat::peer::envelope::Answer;
use crate::chat::peer::gate;
use crate::contacts::access::require_user;
use crate::contacts::card::format::SignedCard;
use crate::contacts::card::received::{self, BlockedVault};
use crate::contacts::model::{Origin, OriginKind};
use crate::contacts::share::merge;
use crate::contacts::share::model::ContactShareTarget;
use crate::error::{HaexError, Result};
use crate::sync::keys::hex;

/// What the address book knows of a vault, for haex-vault's own handshakes with its devices.
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

/// The haex-vault functions that look vaults up by their identity: channels between extensions (spec
/// 057) and file shares (spec 056).
const CARD_LOOKUP_FEATURES: [&str; 2] = ["extensionPeers", "fileShares"];

fn invalid(reason: &str) -> HaexError {
    HaexError::ContactsInvalid {
        reason: reason.to_owned(),
    }
}

impl ContactsService {
    /// The stored card of `vault`, for a handshake with one of its devices. Only the haex-vault
    /// functions of [`CARD_LOOKUP_FEATURES`] ask; nothing of it reaches an extension.
    pub async fn card_of_vault(&self, caller: &Caller, vault: [u8; 32]) -> Result<VaultCard> {
        match caller {
            Caller::Internal { feature } if CARD_LOOKUP_FEATURES.contains(feature) => {}
            _ => return Err(HaexError::ContactsForbidden),
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

    /// Blocks or unblocks a vault (FR-039); `vault_pubkey` is 64 hex characters. A vault a contact
    /// holds blocks that contact with all its vaults ([`Self::block_contact`]); unblocking restores
    /// what the block hid (spec 099 research R13).
    pub async fn set_blocked(
        &self,
        caller: &Caller,
        vault_pubkey: String,
        blocked: bool,
    ) -> Result<()> {
        require_user(caller)?;
        let key = vault_pubkey.trim().to_ascii_lowercase();
        let vault = data_encoding::HEXLOWER
            .decode(key.as_bytes())
            .ok()
            .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
            .ok_or_else(|| invalid("not a vault identity"))?;
        let origin = self.card_origin(caller);
        self.db()
            .write(move |tx| {
                if blocked {
                    match received::stored(tx, &vault)? {
                        Some((contact_id, _)) => block_vaults_of(tx, &contact_id)?,
                        None => block_vault(tx, &key)?,
                    }
                } else {
                    unblock(tx, &vault, &origin)?;
                }
                Ok(())
            })
            .await
    }

    /// Blocks a stored contact (spec 099 FR-014, research R14): every vault its cards name goes on
    /// the block list, each with a blocked answer for its devices. The contact is hidden then.
    pub async fn block_contact(&self, caller: &Caller, contact_id: String) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                if received::vaults_of_contact(tx, &contact_id)?.is_empty() {
                    return Err(invalid("the contact has no vault to block").into());
                }
                block_vaults_of(tx, &contact_id)?;
                Ok(())
            })
            .await
    }

    /// The origin of a contact made from a card the person allowed again.
    fn card_origin(&self, caller: &Caller) -> Origin {
        Origin {
            kind: OriginKind::Card,
            device: self.device(),
            caller: super::caller_text(caller),
            account: None,
        }
    }
}

/// Blocks `vault` (hex) and notes the blocked answer for its devices (research R5).
fn block_vault(tx: &mut CrdtTransaction<'_>, vault: &str) -> Result<()> {
    received::block(tx, vault)?;
    gate::write_notice(tx, vault, Answer::Blocked, now_ms())
}

/// Blocks every vault of `contact_id`.
fn block_vaults_of(tx: &mut CrdtTransaction<'_>, contact_id: &str) -> Result<()> {
    for vault in received::vaults_of_contact(tx, contact_id)? {
        block_vault(tx, &vault)?;
    }
    Ok(())
}

/// Unblocks `vault` (research R13): for a hidden contact every vault of it, for a declined stranger
/// a contact made from the card its entry kept. Each vault whose entry goes gets the accepted
/// answer.
fn unblock(tx: &mut CrdtTransaction<'_>, vault: &[u8; 32], origin: &Origin) -> Result<()> {
    let key = hex(vault);
    let vaults = match received::stored(tx, vault)? {
        Some((contact_id, _)) => received::vaults_of_contact(tx, &contact_id)?,
        None => vec![key.clone()],
    };
    let kept = received::blocked_card(tx, vault)?;
    for vault in vaults {
        if received::unblock(tx, &vault)? {
            gate::write_notice(tx, &vault, Answer::Accepted, now_ms())?;
        }
    }
    if let Some(card) = kept {
        if received::stored(tx, vault)?.is_none() {
            merge::apply(
                tx,
                &card.text(),
                Some(&ContactShareTarget::New),
                &[],
                None,
                origin,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "cards_tests.rs"]
mod tests;

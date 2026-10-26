//! Visitenkarten in the service (spec 051 FR-009 to FR-013, FR-039): the own card and what it
//! carries, reading a card in after a preview, and blocked vaults. Only the user does any of it.
//! A card is checked again on accepting: what the preview showed is never trusted.

use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::{caller_text, Caller, ContactsService};
use crate::contacts::access::require_user;
use crate::contacts::card::format::{self, CardValue, SignedCard};
use crate::contacts::card::own_card::{self, Choice, OwnCard};
use crate::contacts::card::received::{self, BlockedVault};
use crate::contacts::kinds;
use crate::contacts::model::{Contact, Head, Origin, OriginKind};
use crate::contacts::store;
use crate::error::{HolziError, Result};
use crate::passwords::clock;
use crate::storage::query::Query;
use crate::sync::keys;

/// Where an accepted card goes (FR-011).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsCardInto"
)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CardInto {
    New,
    Contact { contact_id: String },
}

/// What a card shows before it is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsCardPreview"
)]
#[serde(rename_all = "camelCase")]
pub struct CardPreview {
    pub name: String,
    pub npub: String,
    pub vault_pubkey: String,
    pub issued_at: String,
    pub devices: u32,
    pub nostr_relays: Vec<String>,
    pub values: Vec<CardPreviewValue>,
    /// The contact that already has this vault's card; only it can take the card (FR-013).
    pub existing_contact: Option<CardContact>,
    pub blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsCardPreviewValue"
)]
#[serde(rename_all = "camelCase")]
pub struct CardPreviewValue {
    pub kind: String,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsCardContact"
)]
#[serde(rename_all = "camelCase")]
pub struct CardContact {
    pub id: String,
    pub name: String,
}

/// Reads a card and checks it against this vault: not the own card, not older than the stored
/// one (FR-011, FR-012). Answers the card and the contact that has the vault's card already.
fn checked(q: &mut impl Query, text: &str) -> Result<(SignedCard, Option<String>)> {
    let card = format::parse(text)?;
    if keys::vault_pubkey(q)? == Some(card.card.vault) {
        return Err(HolziError::CardOwn);
    }
    let existing = match received::stored(q, &card.card.vault)? {
        Some((_, old)) if card.card.issued_at <= old.card.issued_at => {
            return Err(HolziError::CardOlder)
        }
        Some((contact, _)) => Some(contact),
        None => None,
    };
    Ok((card, existing))
}

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

/// Adds the card's values a contact does not have yet; values this version refuses are left out.
fn add_values(tx: &mut CrdtTransaction<'_>, contact_id: &str, values: &[CardValue]) -> Result<()> {
    let present = store::values_of(tx, contact_id)?;
    for value in values {
        let known = kinds::is_known(&value.kind) && !kinds::is_holzi_address(&value.kind);
        let valid = kinds::validate(&value.kind, &value.label, &value.value).is_ok();
        let there = present
            .iter()
            .any(|have| have.kind == value.kind && have.value == value.value);
        if known && valid && !there {
            let position = store::next_position(tx, contact_id, &value.kind)?;
            store::insert_value(
                tx,
                contact_id,
                &value.kind,
                &value.label,
                &value.value,
                position,
            )?;
        }
    }
    Ok(())
}

impl ContactsService {
    /// The own card; a main device makes the first one when there is none yet.
    pub async fn own_card(&self, caller: &Caller) -> Result<OwnCard> {
        require_user(caller)?;
        let card = self
            .db()
            .read(|q| own_card::read(q).map_err(Into::into))
            .await?;
        if card.text.is_some() || !card.can_sign {
            return Ok(card);
        }
        self.db()
            .write(|tx| {
                own_card::refresh(tx)?;
                Ok(own_card::read(tx)?)
            })
            .await
    }

    /// Chooses the own contact and the values the card carries; a main device makes a new card.
    pub async fn own_card_set(
        &self,
        caller: &Caller,
        own_contact: Option<String>,
        value_ids: Vec<String>,
    ) -> Result<OwnCard> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                let value_ids = match &own_contact {
                    Some(id) => {
                        let contact = store::load(tx, id)?;
                        if !value_ids
                            .iter()
                            .all(|wanted| contact.values.iter().any(|value| &value.id == wanted))
                        {
                            return Err(invalid("a value is not one of the own contact").into());
                        }
                        value_ids
                    }
                    None => Vec::new(),
                };
                own_card::set_choice(
                    tx,
                    &Choice {
                        own_contact,
                        value_ids,
                    },
                )?;
                own_card::refresh(tx)?;
                Ok(own_card::read(tx)?)
            })
            .await
    }

    /// Checks a card and shows what it carries; nothing is stored (FR-011).
    pub async fn card_preview(&self, caller: &Caller, text: String) -> Result<CardPreview> {
        require_user(caller)?;
        self.db()
            .read(move |q| {
                let (card, existing) = checked(q, &text)?;
                let existing_contact = match existing {
                    Some(id) => Some(CardContact {
                        name: store::load(q, &id)?.name,
                        id,
                    }),
                    None => None,
                };
                Ok(CardPreview {
                    name: card.card.name.clone(),
                    npub: received::npub(&card.card.vault),
                    vault_pubkey: keys::hex(&card.card.vault),
                    issued_at: received::issued_text(&card),
                    devices: u32::try_from(card.card.devices.len()).unwrap_or(u32::MAX),
                    nostr_relays: card.card.nostr_relays.clone(),
                    values: card
                        .card
                        .values
                        .iter()
                        .map(|value| CardPreviewValue {
                            kind: value.kind.clone(),
                            label: value.label.clone(),
                            value: value.value.clone(),
                        })
                        .collect(),
                    existing_contact,
                    blocked: received::is_blocked(q, &card.card.vault)?,
                })
            })
            .await
    }

    /// Stores a card as a new contact or on an existing one, after checking it again. A vault's
    /// card stays on one contact (FR-013); a newer card replaces its addresses (FR-012).
    pub async fn card_accept(
        &self,
        caller: &Caller,
        text: String,
        into: CardInto,
    ) -> Result<Contact> {
        require_user(caller)?;
        let origin = Origin {
            kind: OriginKind::Card,
            device: self.device(),
            caller: caller_text(caller),
            account: None,
        };
        self.db()
            .write(move |tx| {
                let (card, existing) = checked(tx, &text)?;
                let now = clock::now();
                let contact_id = match (into, existing) {
                    (CardInto::Contact { contact_id }, existing) => {
                        if existing.as_ref().is_some_and(|other| other != &contact_id) {
                            return Err(invalid("this vault is on another contact").into());
                        }
                        store::group_ids_if_exists(tx, &contact_id)?
                            .ok_or(HolziError::ContactsNotFound)?;
                        contact_id
                    }
                    (CardInto::New, Some(_)) => {
                        return Err(invalid("this vault is on another contact").into())
                    }
                    (CardInto::New, None) => {
                        let id = Uuid::new_v4().to_string();
                        // Leave display_name unset when the card carries no name, so the list
                        // falls back to the name parts the user enters later instead of being
                        // stuck on the vault's npub (display_name always wins in derived_name).
                        let name = card.card.name.trim();
                        let head = Head {
                            display_name: (!name.is_empty()).then(|| name.to_owned()),
                            ..Head::default()
                        };
                        store::insert(tx, &id, &head, &origin, &now)?;
                        id
                    }
                };
                add_values(tx, &contact_id, &card.card.values)?;
                received::replace(tx, &contact_id, &card)?;
                store::touch(tx, &contact_id, &now)?;
                Ok(store::load(tx, &contact_id)?)
            })
            .await
    }

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

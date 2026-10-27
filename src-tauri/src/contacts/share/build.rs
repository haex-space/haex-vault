//! The dialog's rows for a contact and the text a choice makes (spec 060 FR-008 to FR-017a). The
//! same code serves the own contact and every other: the own one adds a fresh card of the own vault
//! with the devices and relays chosen, a foreign one with confirmed addresses adds its original card
//! unchanged, and a contact without either shares without a card. Nothing is stored.

use super::envelope::{self, ShareEnvelope, ShareValue};
use super::model::{
    ContactShareDevice, ContactShareHead, ContactShareHolzi, ContactShareHolziChoice,
    ContactShareOptions, ContactShareOutput, ContactShareRow, ContactShareSelection,
};
use super::name::name_for;
use crate::contacts::card::format::{self, SignedCard};
use crate::contacts::card::{own_card, received};
use crate::contacts::kinds;
use crate::contacts::model::{Contact, Head};
use crate::contacts::store;
use crate::error::{HolziError, Result};
use crate::storage::query::Query;
use crate::sync::keys;
use crate::sync::keys::hex;
use crate::sync::presence::decode_hex;

fn invalid(reason: &str) -> HolziError {
    HolziError::ShareInvalid {
        reason: reason.to_owned(),
    }
}

/// A contact with what a share of it needs beyond its rows.
struct Subject {
    contact: Contact,
    /// The vault identity the name falls back to: the own one, or that of a stored card.
    vault: Option<[u8; 32]>,
    /// The newest stored card of a foreign contact.
    card: Option<SignedCard>,
}

fn subject(q: &mut impl Query, contact_id: &str) -> Result<Subject> {
    let contact = store::load(q, contact_id)?;
    if contact.is_self {
        let vault = keys::vault_pubkey(q)?;
        return Ok(Subject {
            contact,
            vault,
            card: None,
        });
    }
    let card = received::stored_of_contact(q, contact_id)?;
    Ok(Subject {
        vault: card.as_ref().map(|card| card.card.vault),
        contact,
        card,
    })
}

/// The values that are rows: those of the own contact are never holzi addresses (FR-005), those of
/// another contact may be, as unconfirmed ones (FR-014).
fn rows_of(subject: &Subject) -> Vec<ContactShareRow> {
    subject
        .contact
        .values
        .iter()
        .filter(|value| !(subject.contact.is_self && kinds::is_holzi_address(&value.kind)))
        .map(|value| ContactShareRow {
            id: value.id.clone(),
            kind: value.kind.clone(),
            label: value.label.clone(),
            value: value.value.clone(),
        })
        .collect()
}

fn filled(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
}

fn head_of(head: &Head) -> ContactShareHead {
    ContactShareHead {
        display_name: filled(&head.display_name),
        nickname: filled(&head.nickname),
        organization: filled(&head.organization),
        job_title: filled(&head.job_title),
        note: filled(&head.note),
    }
}

fn has_head(head: &ContactShareHead) -> bool {
    [
        &head.display_name,
        &head.nickname,
        &head.organization,
        &head.job_title,
        &head.note,
    ]
    .into_iter()
    .any(Option::is_some)
}

fn holzi_of(q: &mut impl Query, subject: &Subject) -> Result<ContactShareHolzi> {
    if subject.contact.is_self {
        let Some(vault) = subject.vault else {
            return Ok(ContactShareHolzi::None);
        };
        let can_sign = keys::vault_secret(q)?.is_some();
        let known = if can_sign {
            own_card::addresses(q)?
        } else {
            None
        };
        let (devices, relays) = known.map_or_else(Default::default, |known| {
            let devices = known
                .devices
                .into_iter()
                .zip(known.device_uuids)
                .map(|(device, uuid)| ContactShareDevice {
                    device_pubkey: hex(&device.device_pubkey),
                    vault_device_uuid: uuid,
                    iroh_relay: device.iroh_relay,
                })
                .collect();
            (devices, known.nostr_relays)
        });
        return Ok(ContactShareHolzi::Own {
            can_sign,
            npub: received::npub(&vault),
            devices,
            relays,
        });
    }
    Ok(match &subject.card {
        Some(card) => ContactShareHolzi::Card {
            npub: received::npub(&card.card.vault),
            devices: u32::try_from(card.card.devices.len()).unwrap_or(u32::MAX),
            relays: u32::try_from(card.card.nostr_relays.len()).unwrap_or(u32::MAX),
            issued_at: received::issued_text(card),
        },
        None => ContactShareHolzi::None,
    })
}

/// What the dialog shows for the contact, everything chosen.
pub fn options(q: &mut impl Query, contact_id: &str) -> Result<ContactShareOptions> {
    let subject = subject(q, contact_id)?;
    let rows = rows_of(&subject);
    let head = head_of(&subject.contact.head);
    let holzi = holzi_of(q, &subject)?;
    let (name, _) = name_for(&subject.contact.head, subject.vault);
    let card_goes = matches!(
        holzi,
        ContactShareHolzi::Card { .. } | ContactShareHolzi::Own { can_sign: true, .. }
    );
    Ok(ContactShareOptions {
        can_share: !name.is_empty() || !rows.is_empty() || has_head(&head) || card_goes,
        rows,
        head,
        holzi,
        name,
    })
}

fn keys_of(devices: &[String]) -> Result<Vec<[u8; 32]>> {
    devices
        .iter()
        .map(|device| {
            decode_hex(device)
                .ok()
                .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                .ok_or_else(|| invalid("a device key is not valid"))
        })
        .collect()
}

/// The embedded card a choice makes: none, the stored original of a foreign contact, or a fresh one
/// of the own vault (none on a device without the vault secret, FR-016).
fn card_of(
    q: &mut impl Query,
    subject: &Subject,
    choice: &ContactShareHolziChoice,
) -> Result<Option<Vec<u8>>> {
    match choice {
        ContactShareHolziChoice::None => Ok(None),
        ContactShareHolziChoice::Card => subject
            .card
            .as_ref()
            .map(|card| Some(card.bytes.clone()))
            .ok_or_else(|| invalid("the contact has no confirmed holzi addresses")),
        ContactShareHolziChoice::Own { devices, relays } => {
            if !subject.contact.is_self {
                return Err(invalid("only the own contact shares the own addresses"));
            }
            let card = own_card::build_for_share(q, &keys_of(devices)?, relays)?;
            Ok(card.map(|card| card.bytes))
        }
    }
}

/// The text of the share the choice makes. A choice the dialog does not offer (a value of another
/// contact, a device that is not the vault's) is [`HolziError::ShareInvalid`]; nothing to share is
/// too (FR-017); a text over the limit is [`HolziError::ShareTooLarge`].
pub fn build(
    q: &mut impl Query,
    contact_id: &str,
    selection: &ContactShareSelection,
) -> Result<ContactShareOutput> {
    let subject = subject(q, contact_id)?;
    let rows = rows_of(&subject);
    let mut values = Vec::new();
    for wanted in &selection.values {
        if values
            .iter()
            .any(|row: &&ContactShareRow| &row.id == wanted)
        {
            continue;
        }
        values.push(
            rows.iter()
                .find(|row| &row.id == wanted)
                .ok_or_else(|| invalid("a detail does not belong to the contact"))?,
        );
    }
    // In the order of the contact, not of the selection.
    values.sort_by_key(|row| rows.iter().position(|other| other.id == row.id));

    let (name, name_parts) = name_for(&subject.contact.head, subject.vault);
    let head = head_of(&subject.contact.head);
    let mut shared = ShareEnvelope::new(name);
    shared.name_parts = name_parts;
    shared.head.display_name = head.display_name.filter(|_| selection.display_name);
    shared.head.nickname = head.nickname.filter(|_| selection.nickname);
    shared.head.organization = head.organization.filter(|_| selection.organization);
    shared.head.job_title = head.job_title.filter(|_| selection.job_title);
    shared.head.note = head.note.filter(|_| selection.note);
    shared.values = values
        .into_iter()
        .map(|row| ShareValue {
            kind: row.kind.clone(),
            label: row.label.clone(),
            value: row.value.clone(),
        })
        .collect();
    shared.card = card_of(q, &subject, &selection.holzi)?;

    let nothing = shared.name.is_empty()
        && shared.values.is_empty()
        && shared.card.is_none()
        && !has_head(&ContactShareHead {
            display_name: shared.head.display_name.clone(),
            nickname: shared.head.nickname.clone(),
            organization: shared.head.organization.clone(),
            job_title: shared.head.job_title.clone(),
            note: shared.head.note.clone(),
        });
    if nothing {
        return Err(invalid("nothing to share"));
    }
    let link = envelope::encode(&shared)?;
    let qr_svg = envelope::fits_qr(&link)
        .then(|| format::qr_svg(&link))
        .transpose()?;
    Ok(ContactShareOutput {
        size: u32::try_from(link.chars().count()).unwrap_or(u32::MAX),
        link,
        qr_svg,
    })
}

#[cfg(test)]
#[path = "build_tests.rs"]
mod tests;

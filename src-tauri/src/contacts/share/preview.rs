//! What reading a share in would do (spec 060 FR-021 to FR-025a): the share is decoded and checked,
//! compared with the target, and every item gets its state and the choices the person may make.
//! Nothing is written. [`plan`] is also what [`super::merge`] runs again on accepting, from the text
//! alone, so what the window showed is never what is trusted.

use sha2::{Digest, Sha256};

use super::envelope::{self, Decoded};
use super::model::{
    ContactShareAddresses, ContactShareCandidate, ContactShareChoice, ContactShareExisting,
    ContactShareIdentity, ContactShareItem, ContactShareItemKind, ContactSharePreview,
    ContactShareReason, ContactShareState, ContactShareTarget,
};
use crate::contacts::card::received;
use crate::contacts::kinds;
use crate::contacts::model::{Contact, Head};
use crate::contacts::{ids, store};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;
use crate::sync::keys;

/// What an item writes when it is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Payload {
    /// A field the contact has once: the column and the text.
    Head { column: &'static str, text: String },
    Value {
        kind: String,
        label: String,
        value: String,
    },
    /// The card of the share, which `Decoded` holds.
    Holzi,
}

/// The preview with what applying it needs.
pub(super) struct Plan {
    pub preview: ContactSharePreview,
    /// One payload per item of the preview, in its order.
    pub payloads: Vec<Payload>,
    pub decoded: Decoded,
    /// The contact that takes the share, if it exists.
    pub target: Option<String>,
    /// The display name a new contact gets when the share gives no name parts.
    pub new_display_name: Option<String>,
}

fn invalid(reason: &str) -> HolziError {
    HolziError::ShareInvalid {
        reason: reason.to_owned(),
    }
}

fn filled(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// A phone number as its digits, so `0170 123-45 67` and `01701234567` are the same.
fn digits(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

/// The fields a contact has once, as the share names them: the item, the column, the text of the
/// share and what the target says now.
fn head_fields(
    decoded: &Decoded,
    target: Option<&Head>,
) -> Vec<(ContactShareItemKind, &'static str, String, Option<String>)> {
    let parts = &decoded.envelope.name_parts;
    let head = &decoded.envelope.head;
    let part = |field: &str| ContactShareItemKind::NamePart {
        field: field.to_owned(),
    };
    let rows = [
        (part("prefix"), "name_prefix", &parts.prefix),
        (part("given"), "given_name", &parts.given),
        (part("middle"), "middle_name", &parts.middle),
        (part("family"), "family_name", &parts.family),
        (part("suffix"), "name_suffix", &parts.suffix),
        (
            ContactShareItemKind::DisplayName,
            "display_name",
            &head.display_name,
        ),
        (ContactShareItemKind::Nickname, "nickname", &head.nickname),
        (
            ContactShareItemKind::Organization,
            "organization",
            &head.organization,
        ),
        (ContactShareItemKind::JobTitle, "job_title", &head.job_title),
        (ContactShareItemKind::Note, "note", &head.note),
    ];
    rows.into_iter()
        .filter_map(|(kind, column, incoming)| {
            let text = filled(incoming)?;
            let current = target.and_then(|head| current_of(head, column));
            Some((kind, column, text, current))
        })
        .collect()
}

fn current_of(head: &Head, column: &str) -> Option<String> {
    let field = match column {
        "display_name" => &head.display_name,
        "given_name" => &head.given_name,
        "family_name" => &head.family_name,
        "middle_name" => &head.middle_name,
        "name_prefix" => &head.name_prefix,
        "name_suffix" => &head.name_suffix,
        "nickname" => &head.nickname,
        "organization" => &head.organization,
        "job_title" => &head.job_title,
        "note" => &head.note,
        _ => return None,
    };
    filled(field)
}

/// A snapshot of what the preview was computed against: the target's fields, values and the cards
/// stored on it. Opaque to the window; accepting must give it back unchanged (FR-024d).
fn revision_of(q: &mut impl Query, contact: &Contact) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut put = |text: &str| {
        hasher.update(text.len().to_le_bytes());
        hasher.update(text.as_bytes());
    };
    put(&contact.id);
    put(&contact.updated_at);
    for column in [
        "display_name",
        "given_name",
        "family_name",
        "middle_name",
        "name_prefix",
        "name_suffix",
        "nickname",
        "organization",
        "job_title",
        "note",
    ] {
        put(current_of(&contact.head, column).as_deref().unwrap_or(""));
    }
    for value in &contact.values {
        put(&value.id);
        put(&value.kind);
        put(&value.label);
        put(&value.value);
    }
    for hash in received::hashes_of_contact(q, &contact.id)? {
        put(&hash);
    }
    Ok(keys::hex(&hasher.finalize()))
}

fn count(items: usize) -> u32 {
    u32::try_from(items).unwrap_or(u32::MAX)
}

/// Contacts that share a phone number or an e-mail address with the share, as hints (FR-024).
fn hints(q: &mut impl Query, decoded: &Decoded) -> Result<Vec<ContactShareCandidate>> {
    let phones: Vec<String> = decoded
        .envelope
        .values
        .iter()
        .filter(|value| value.kind == kinds::PHONE)
        .map(|value| digits(&value.value))
        .filter(|number| !number.is_empty())
        .collect();
    let emails: Vec<String> = decoded
        .envelope
        .values
        .iter()
        .filter(|value| value.kind == kinds::EMAIL)
        .map(|value| value.value.trim().to_lowercase())
        .collect();
    if phones.is_empty() && emails.is_empty() {
        return Ok(Vec::new());
    }
    Ok(store::headers(q)?
        .into_iter()
        .filter(|header| !header.is_self)
        .filter(|header| {
            header
                .phones
                .iter()
                .any(|phone| phones.contains(&digits(phone)))
                || header
                    .emails
                    .iter()
                    .any(|email| emails.contains(&email.trim().to_lowercase()))
        })
        .map(|header| ContactShareCandidate {
            id: header.id,
            name: header.name,
            reason: ContactShareReason::Hint,
        })
        .collect())
}

/// Reads the text and compares it with the target. `requested` is the person's choice of target;
/// without one, a vault that is on a contact already selects it, else a new contact is made.
pub(super) fn plan(
    q: &mut impl Query,
    text: &str,
    requested: Option<&ContactShareTarget>,
) -> Result<Plan> {
    let own = keys::vault_pubkey(q)?.unwrap_or([0; 32]);
    let decoded = envelope::decode(text, &own)?;
    let card = decoded.card.clone();
    let vault = card.as_ref().map(|card| card.card.vault);
    let blocked = match vault {
        Some(vault) => received::is_blocked(q, &vault)?,
        None => false,
    };

    // A vault identity stands on one contact (spec 051 FR-013): its contact is the only target.
    let vault_contact = match vault {
        Some(vault) => received::stored(q, &vault)?.map(|(contact_id, _)| contact_id),
        None => None,
    };
    let target_id = match (&vault_contact, requested) {
        (Some(on), None) => Some(on.clone()),
        (Some(on), Some(ContactShareTarget::Contact { id })) if id == on => Some(on.clone()),
        (Some(_), Some(_)) => return Err(invalid("this vault is on another contact")),
        (None, Some(ContactShareTarget::Contact { id })) => Some(id.clone()),
        (None, _) => None,
    };
    let target = match &target_id {
        Some(id) => {
            if ids::is_self(id) {
                return Err(invalid("the own contact cannot take a share"));
            }
            store::group_ids_if_exists(q, id)?.ok_or(HolziError::ContactsNotFound)?;
            Some(store::load(q, id)?)
        }
        None => None,
    };

    let identity = match (&card, &target) {
        (None, _) => ContactShareIdentity::None,
        (Some(_), None) => ContactShareIdentity::New,
        (Some(card), Some(contact)) => match received::stored_of_contact(q, &contact.id)? {
            None => ContactShareIdentity::New,
            Some(held) if held.card.vault == card.card.vault => ContactShareIdentity::Matches,
            Some(held) => ContactShareIdentity::Mismatch {
                existing_npub: received::npub(&held.card.vault),
            },
        },
    };

    let mut items: Vec<ContactShareItem> = Vec::new();
    let mut payloads: Vec<Payload> = Vec::new();
    let mut push = |item: ContactShareItem, payload: Payload| {
        items.push(ContactShareItem {
            id: count(payloads.len()),
            ..item
        });
        payloads.push(payload);
    };

    for (kind, column, text, current) in head_fields(&decoded, target.as_ref().map(|c| &c.head)) {
        let (state, choices, default) = match &current {
            None => (ContactShareState::New, Vec::new(), None),
            Some(held) if held.as_str() == text => (ContactShareState::Same, Vec::new(), None),
            Some(_) => (
                ContactShareState::Conflict,
                vec![ContactShareChoice::Keep, ContactShareChoice::Update],
                Some(ContactShareChoice::Keep),
            ),
        };
        push(
            ContactShareItem {
                id: 0,
                item: kind,
                value_kind: None,
                label: String::new(),
                value: text.clone(),
                state,
                choices,
                default,
                existing: Vec::new(),
                current,
                addresses: None,
            },
            Payload::Head { column, text },
        );
    }

    let mut seen: Vec<(&str, &str)> = Vec::new();
    for value in &decoded.envelope.values {
        if seen.contains(&(value.kind.as_str(), value.value.as_str())) {
            continue;
        }
        seen.push((value.kind.as_str(), value.value.as_str()));
        let held = target
            .as_ref()
            .map(|contact| contact.values.as_slice())
            .unwrap_or(&[]);
        let same = held
            .iter()
            .any(|have| have.kind == value.kind && have.value == value.value);
        let existing: Vec<ContactShareExisting> = if kinds::is_known(&value.kind) {
            held.iter()
                .filter(|have| have.kind == value.kind)
                .map(|have| ContactShareExisting {
                    id: have.id.clone(),
                    kind: have.kind.clone(),
                    label: have.label.clone(),
                    value: have.value.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };
        let (state, choices, default) = if same {
            (ContactShareState::Same, Vec::new(), None)
        } else {
            let mut choices = vec![ContactShareChoice::Add];
            if !existing.is_empty() {
                choices.push(ContactShareChoice::Update);
            }
            choices.push(ContactShareChoice::Skip);
            (
                ContactShareState::New,
                choices,
                Some(ContactShareChoice::Add),
            )
        };
        push(
            ContactShareItem {
                id: 0,
                item: ContactShareItemKind::Value,
                value_kind: Some(value.kind.clone()),
                label: value.label.clone(),
                value: value.value.clone(),
                state,
                choices,
                default,
                existing: if same { Vec::new() } else { existing },
                current: None,
                addresses: None,
            },
            Payload::Value {
                kind: value.kind.clone(),
                label: value.label.clone(),
                value: value.value.clone(),
            },
        );
    }

    if let Some(card) = &card {
        let stored = received::addresses(q, &card.card.vault)?;
        let new_devices = card
            .card
            .devices
            .iter()
            .filter(|device| {
                !stored
                    .devices
                    .iter()
                    .any(|known| known.endpoint_id == device.endpoint_id)
            })
            .count();
        let new_relays = card
            .card
            .nostr_relays
            .iter()
            .filter(|relay| !stored.nostr_relays.contains(relay))
            .count();
        let known_vault = received::stored(q, &card.card.vault)?.is_some();
        // An old card text that is newer than what is stored may still replace it, to drop stale
        // addresses (spec 051 FR-012), so it is asked about even when it adds nothing.
        let newer_text = decoded.legacy_card
            && received::stored(q, &card.card.vault)?
                .is_some_and(|(_, old)| card.card.issued_at > old.card.issued_at);
        // A vault that is known and gains nothing is not asked about (FR-025).
        let same = known_vault && new_devices == 0 && new_relays == 0 && !newer_text;
        let (state, choices, default) = if same {
            (ContactShareState::Same, Vec::new(), None)
        } else {
            let mut choices = vec![ContactShareChoice::Add, ContactShareChoice::Skip];
            if decoded.legacy_card && known_vault {
                choices.push(ContactShareChoice::Replace);
            }
            (
                ContactShareState::New,
                choices,
                Some(ContactShareChoice::Add),
            )
        };
        push(
            ContactShareItem {
                id: 0,
                item: ContactShareItemKind::HolziAddresses,
                value_kind: None,
                label: String::new(),
                value: received::npub(&card.card.vault),
                state,
                choices,
                default,
                existing: Vec::new(),
                current: None,
                addresses: Some(ContactShareAddresses {
                    npub: received::npub(&card.card.vault),
                    devices: count(card.card.devices.len()),
                    relays: count(card.card.nostr_relays.len()),
                    new_devices: count(new_devices),
                    new_relays: count(new_relays),
                }),
            },
            Payload::Holzi,
        );
    }

    let name = decoded.envelope.name.clone();
    let has_parts = head_fields_have_parts(&decoded);
    if items.is_empty() && name.is_empty() {
        return Err(invalid("there is nothing in the share"));
    }
    let new_display_name = (target.is_none()
        && !has_parts
        && filled(&decoded.envelope.head.display_name).is_none()
        && !name.trim().is_empty())
    .then(|| name.trim().to_owned());

    let candidates = match (&vault_contact, &target) {
        (Some(on), Some(contact)) if &contact.id == on => vec![ContactShareCandidate {
            id: contact.id.clone(),
            name: contact.name.clone(),
            reason: ContactShareReason::Vault,
        }],
        _ => hints(q, &decoded)?,
    };
    let npub = card.as_ref().map(|card| received::npub(&card.card.vault));
    let (target_value, target_name, target_revision) = match &target {
        Some(contact) => (
            ContactShareTarget::Contact {
                id: contact.id.clone(),
            },
            Some(contact.name.clone()),
            Some(revision_of(q, contact)?),
        ),
        None => (ContactShareTarget::New, None, None),
    };
    let reachable = card
        .as_ref()
        .is_some_and(|card| !card.card.devices.is_empty() || !card.card.nostr_relays.is_empty());
    let preview = ContactSharePreview {
        name_is_npub: npub.as_deref() == Some(name.as_str()),
        name,
        npub,
        vault_pubkey: vault.map(|vault| keys::hex(&vault)),
        target: target_value,
        target_name,
        target_revision,
        candidates,
        identity,
        items,
        reachable,
        skipped: count(decoded.skipped),
        blocked,
        legacy_card: decoded.legacy_card,
    };
    Ok(Plan {
        preview,
        payloads,
        target: target.map(|contact| contact.id),
        decoded,
        new_display_name,
    })
}

fn head_fields_have_parts(decoded: &Decoded) -> bool {
    let parts = &decoded.envelope.name_parts;
    [
        &parts.prefix,
        &parts.given,
        &parts.middle,
        &parts.family,
        &parts.suffix,
    ]
    .into_iter()
    .any(|part| filled(part).is_some())
}

/// What reading the share in would do; read-only.
pub fn preview(
    q: &mut impl Query,
    text: &str,
    target: Option<&ContactShareTarget>,
) -> Result<ContactSharePreview> {
    Ok(plan(q, text, target)?.preview)
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;

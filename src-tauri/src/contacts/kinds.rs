//! The kinds of contact values (spec 051 FR-003, FR-007, FR-008, FR-014; data-model.md). The set is
//! open: a later version adds kinds without a migration, and a value of a kind this version does
//! not know is kept as it is and shown read-only. Only the kinds the phone's address book knows go
//! to a phone; the holzi addresses and custom values never do.

use serde_json::{Map, Value as Json};

pub const PHONE: &str = "phone";
pub const EMAIL: &str = "email";
pub const ADDRESS: &str = "address";
pub const WEBSITE: &str = "website";
pub const IM: &str = "im";
pub const SIP: &str = "sip";
pub const EVENT: &str = "event";
pub const RELATION: &str = "relation";
pub const CUSTOM: &str = "custom";
pub const HOLZI_VAULT: &str = "holzi_vault";
pub const HOLZI_NOSTR_RELAY: &str = "holzi_nostr_relay";
pub const HOLZI_IROH: &str = "holzi_iroh";

/// Every kind this version knows.
pub const KNOWN: [&str; 12] = [
    PHONE,
    EMAIL,
    ADDRESS,
    WEBSITE,
    IM,
    SIP,
    EVENT,
    RELATION,
    CUSTOM,
    HOLZI_VAULT,
    HOLZI_NOSTR_RELAY,
    HOLZI_IROH,
];

/// The parts of an address; the value of an `address` is a JSON object with some of them.
pub const ADDRESS_PARTS: [&str; 7] = [
    "street",
    "pobox",
    "neighborhood",
    "city",
    "region",
    "postcode",
    "country",
];

/// Labels any kind takes; `custom:<text>` is a label of the user's own.
const LABELS: [&str; 4] = ["home", "work", "mobile", "other"];

/// Labels a kind takes beyond [`LABELS`], as the phone's address book has them.
fn kind_labels(kind: &str) -> &'static [&'static str] {
    match kind {
        PHONE => &[
            "main",
            "fax_work",
            "fax_home",
            "pager",
            "car",
            "company_main",
        ],
        EVENT => &["birthday", "anniversary"],
        RELATION => &[
            "assistant",
            "brother",
            "child",
            "domestic_partner",
            "father",
            "friend",
            "manager",
            "mother",
            "parent",
            "partner",
            "referred_by",
            "relative",
            "sister",
            "spouse",
        ],
        _ => &[],
    }
}

/// Most characters of one value.
pub const MAX_VALUE_CHARS: usize = 4096;

/// Most characters of a label of the user's own.
const MAX_CUSTOM_LABEL_CHARS: usize = 100;

pub fn is_known(kind: &str) -> bool {
    KNOWN.contains(&kind)
}

/// Whether values of this kind are holzi addresses (FR-008).
pub fn is_holzi_address(kind: &str) -> bool {
    matches!(kind, HOLZI_VAULT | HOLZI_NOSTR_RELAY | HOLZI_IROH)
}

/// Whether values of this kind go to a phone's address book (FR-014).
pub fn goes_to_phone(kind: &str) -> bool {
    matches!(
        kind,
        PHONE | EMAIL | ADDRESS | WEBSITE | IM | SIP | EVENT | RELATION
    )
}

/// Checks a value of a known kind before it is stored; the error names the reason for the window.
/// Values of unknown kinds are never written by this version, so they are not checked here.
pub fn validate(kind: &str, label: &str, value: &str) -> Result<(), String> {
    if !is_known(kind) {
        return Err(format!("unknown kind {kind}"));
    }
    validate_label(kind, label)?;
    if value.trim().is_empty() {
        return Err("empty value".to_owned());
    }
    if value.chars().count() > MAX_VALUE_CHARS {
        return Err("value too long".to_owned());
    }
    match kind {
        EMAIL if !value.contains('@') => Err("not an e-mail address".to_owned()),
        ADDRESS => validate_address(value),
        IM => validate_im(value),
        EVENT => validate_event(value),
        HOLZI_VAULT => validate_npub(value),
        HOLZI_NOSTR_RELAY if !crate::sync::servers::is_nostr_relay(value) => {
            Err("not a ws:// or wss:// address".to_owned())
        }
        HOLZI_IROH => validate_iroh(value),
        _ => Ok(()),
    }
}

fn validate_label(kind: &str, label: &str) -> Result<(), String> {
    if let Some(own) = label.strip_prefix("custom:") {
        let own = own.trim();
        return if own.is_empty() || own.chars().count() > MAX_CUSTOM_LABEL_CHARS {
            Err("invalid label".to_owned())
        } else {
            Ok(())
        };
    }
    if kind == CUSTOM {
        // A custom value is named by the user: its label is always `custom:<name>`.
        return Err("a custom value needs a name".to_owned());
    }
    if LABELS.contains(&label) || kind_labels(kind).contains(&label) {
        Ok(())
    } else {
        Err(format!("invalid label {label}"))
    }
}

fn object(value: &str) -> Result<Map<String, Json>, String> {
    match serde_json::from_str::<Json>(value) {
        Ok(Json::Object(map)) => Ok(map),
        _ => Err("not a JSON object".to_owned()),
    }
}

fn text_field<'a>(map: &'a Map<String, Json>, key: &str) -> Result<Option<&'a str>, String> {
    match map.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::String(text)) => Ok(Some(text.as_str())),
        Some(_) => Err(format!("{key} is not text")),
    }
}

fn validate_address(value: &str) -> Result<(), String> {
    let map = object(value)?;
    if let Some(key) = map
        .keys()
        .find(|key| !ADDRESS_PARTS.contains(&key.as_str()))
    {
        return Err(format!("unknown address part {key}"));
    }
    let mut any = false;
    for part in ADDRESS_PARTS {
        any |= text_field(&map, part)?.is_some_and(|text| !text.trim().is_empty());
    }
    if any {
        Ok(())
    } else {
        Err("empty address".to_owned())
    }
}

fn validate_im(value: &str) -> Result<(), String> {
    let map = object(value)?;
    let service = text_field(&map, "service")?.unwrap_or_default();
    let handle = text_field(&map, "handle")?.unwrap_or_default();
    if service.trim().is_empty() || handle.trim().is_empty() {
        Err("a messenger needs a service and a handle".to_owned())
    } else {
        Ok(())
    }
}

/// `YYYY-MM-DD`, or `--MM-DD` for a date without a year (as vCard and Android have it).
fn validate_event(value: &str) -> Result<(), String> {
    let map = object(value)?;
    let date = text_field(&map, "date")?.unwrap_or_default();
    let (year, month_day) = match date.strip_prefix("--") {
        Some(rest) => (None, rest),
        None => match date.split_once('-') {
            Some((year, rest)) => (Some(year), rest),
            None => return Err("not a date".to_owned()),
        },
    };
    let year_ok = year.is_none_or(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()));
    let Some((month, day)) = month_day.split_once('-') else {
        return Err("not a date".to_owned());
    };
    let month: u32 = month.parse().map_err(|_| "not a date".to_owned())?;
    let day: u32 = day.parse().map_err(|_| "not a date".to_owned())?;
    if year_ok && (1..=12).contains(&month) && (1..=31).contains(&day) {
        Ok(())
    } else {
        Err("not a date".to_owned())
    }
}

fn validate_npub(value: &str) -> Result<(), String> {
    use nostr::nips::nip19::FromBech32;
    if value.starts_with("npub1") && nostr::key::PublicKey::from_bech32(value).is_ok() {
        Ok(())
    } else {
        Err("not an npub".to_owned())
    }
}

/// 64 hex characters of an iroh endpoint id, optionally `@<iroh relay address>`.
fn validate_iroh(value: &str) -> Result<(), String> {
    let (id, relay) = match value.split_once('@') {
        Some((id, relay)) => (id, Some(relay)),
        None => (value, None),
    };
    if id.len() != 64 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("not an iroh endpoint id".to_owned());
    }
    match relay {
        Some(relay) if !crate::sync::servers::is_iroh_relay(relay) => {
            Err("not an iroh server address".to_owned())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "kinds_tests.rs"]
mod tests;

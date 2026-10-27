//! The envelope of a share and its text (spec 060, contracts/share-format.md):
//! `haex-vault://contact/<base64url(postcard(ShareEnvelope))>`, one text for link, clipboard and
//! QR code. The envelope carries no signature; only the card in it proves anything, and only the
//! holzi addresses. The old `holzi:card:` text is read as an envelope too.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::contacts::card::format::{self, SignedCard};
use crate::contacts::kinds;
use crate::error::{HolziError, Result};

/// What a share's text starts with (spec 054: the app's name as the scheme).
pub const PREFIX_LINK: &str = "haex-vault://contact/";
/// The most characters of a share's text; a bigger one is not given out (FR-019).
pub const MAX_SHARE_TEXT: usize = 16_384;
/// The most characters of a text that goes into a QR code (FR-019).
pub const QR_MAX: usize = 2_048;
/// The most details in one envelope.
pub const MAX_VALUES: usize = 200;
/// The most characters of a kind, label, name part, display name, nickname, organization or title.
pub const MAX_STRING: usize = format::MAX_STRING;
/// The most characters of the note.
pub const MAX_NOTE: usize = kinds::MAX_VALUE_CHARS;
/// The only version this one reads and writes.
pub const VERSION: u8 = 1;

/// The name parts a share has, one by one so the preview can choose per field (FR-024c).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameParts {
    pub prefix: Option<String>,
    pub given: Option<String>,
    pub middle: Option<String>,
    pub family: Option<String>,
    pub suffix: Option<String>,
}

/// The head fields beside the name parts, as far as they were chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareHead {
    pub display_name: Option<String>,
    pub nickname: Option<String>,
    pub organization: Option<String>,
    pub job_title: Option<String>,
    pub note: Option<String>,
}

/// One chosen detail. The kind is a text, so a kind this version does not know survives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareValue {
    pub kind: String,
    pub label: String,
    pub value: String,
}

/// What a share holds. Field order is part of the format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareEnvelope {
    pub version: u8,
    /// The name of FR-011: name parts, else the `npub`, else empty.
    pub name: String,
    pub name_parts: NameParts,
    pub head: ShareHead,
    pub values: Vec<ShareValue>,
    /// A card of spec 051 as it was signed (`len ‖ postcard ‖ signature`).
    pub card: Option<Vec<u8>>,
}

impl ShareEnvelope {
    /// An envelope of this version without details.
    pub fn new(name: String) -> Self {
        Self {
            version: VERSION,
            name,
            name_parts: NameParts::default(),
            head: ShareHead::default(),
            values: Vec::new(),
            card: None,
        }
    }
}

/// An envelope as read, with what reading left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub envelope: ShareEnvelope,
    /// The card of the envelope, checked: format, limits and signature hold.
    pub card: Option<SignedCard>,
    /// Details left out: a known kind that does not validate.
    pub skipped: usize,
    /// It came as a `holzi:card:` text, which also replaces an older card of the vault (051 FR-012).
    pub legacy_card: bool,
}

fn invalid(reason: &str) -> HolziError {
    HolziError::ShareInvalid {
        reason: reason.to_owned(),
    }
}

fn longest(texts: impl IntoIterator<Item = Option<impl AsRef<str>>>, most: usize) -> bool {
    texts
        .into_iter()
        .flatten()
        .any(|text| text.as_ref().chars().count() > most)
}

/// The limits an envelope keeps, on writing and on reading.
fn check_limits(envelope: &ShareEnvelope) -> Result<()> {
    if envelope.version != VERSION {
        return Err(invalid("unknown version"));
    }
    if envelope.values.len() > MAX_VALUES {
        return Err(invalid("too many details"));
    }
    let parts = &envelope.name_parts;
    let head = &envelope.head;
    let strings = [
        Some(&envelope.name),
        parts.prefix.as_ref(),
        parts.given.as_ref(),
        parts.middle.as_ref(),
        parts.family.as_ref(),
        parts.suffix.as_ref(),
        head.display_name.as_ref(),
        head.nickname.as_ref(),
        head.organization.as_ref(),
        head.job_title.as_ref(),
    ];
    if longest(strings, MAX_STRING) || longest([head.note.as_ref()], MAX_NOTE) {
        return Err(invalid("a text is too long"));
    }
    for value in &envelope.values {
        if value.kind.chars().count() > MAX_STRING
            || value.label.chars().count() > MAX_STRING
            || value.value.chars().count() > kinds::MAX_VALUE_CHARS
        {
            return Err(invalid("a text is too long"));
        }
    }
    Ok(())
}

/// The text of an envelope. Over [`MAX_SHARE_TEXT`] characters is [`HolziError::ShareTooLarge`];
/// nothing is cut off.
pub fn encode(envelope: &ShareEnvelope) -> Result<String> {
    check_limits(envelope)?;
    let bytes = postcard::to_stdvec(envelope).map_err(|_| invalid("not encodable"))?;
    let text = format!("{PREFIX_LINK}{}", URL_SAFE_NO_PAD.encode(bytes));
    if text.len() > MAX_SHARE_TEXT {
        return Err(HolziError::ShareTooLarge);
    }
    Ok(text)
}

/// Whether the text fits a QR code (FR-019).
pub fn fits_qr(text: &str) -> bool {
    text.len() <= QR_MAX
}

/// Reads a text: a share, or a `holzi:card:` of spec 051. `own_vault` is this vault's identity; a
/// card of it is [`HolziError::CardOwn`]. Only a form error rejects the envelope (FR-022); a detail
/// that does not validate is left out and counted.
pub fn decode(text: &str, own_vault: &[u8; 32]) -> Result<Decoded> {
    let text = text.trim();
    if text.starts_with(format::PREFIX) {
        return legacy(text, own_vault);
    }
    if text.len() > MAX_SHARE_TEXT {
        return Err(invalid("too long"));
    }
    let encoded = text
        .strip_prefix(PREFIX_LINK)
        .ok_or_else(|| invalid("not a holzi contact"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| invalid("not base64url"))?;
    let (mut envelope, rest) = postcard::take_from_bytes::<ShareEnvelope>(&bytes)
        .map_err(|_| invalid("not a holzi contact"))?;
    if !rest.is_empty() {
        return Err(invalid("trailing bytes"));
    }
    check_limits(&envelope)?;
    let card = envelope
        .card
        .clone()
        .map(format::from_bytes)
        .transpose()?
        .map(|card| own_check(card, own_vault))
        .transpose()?;
    let before = envelope.values.len();
    envelope.values.retain(valid_value);
    Ok(Decoded {
        skipped: before - envelope.values.len(),
        envelope,
        card,
        legacy_card: false,
    })
}

fn own_check(card: SignedCard, own_vault: &[u8; 32]) -> Result<SignedCard> {
    if &card.card.vault == own_vault {
        return Err(HolziError::CardOwn);
    }
    Ok(card)
}

/// A known kind must validate; an unknown one stays as it is. A holzi address as a detail is an
/// ordinary, unconfirmed value (FR-014): it proves nothing, only the signed card does.
fn valid_value(value: &ShareValue) -> bool {
    !kinds::is_known(&value.kind)
        || kinds::validate(&value.kind, &value.label, &value.value).is_ok()
}

/// A `holzi:card:` text as an envelope: the card's name, its details, and the card itself.
fn legacy(text: &str, own_vault: &[u8; 32]) -> Result<Decoded> {
    let card = own_check(format::parse(text)?, own_vault)?;
    let mut envelope = ShareEnvelope::new(card.card.name.clone());
    envelope.values = card
        .card
        .values
        .iter()
        .map(|value| ShareValue {
            kind: value.kind.clone(),
            label: value.label.clone(),
            value: value.value.clone(),
        })
        .collect();
    envelope.card = Some(card.bytes.clone());
    let before = envelope.values.len();
    envelope.values.retain(valid_value);
    Ok(Decoded {
        skipped: before - envelope.values.len(),
        envelope,
        card: Some(card),
        legacy_card: true,
    })
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;

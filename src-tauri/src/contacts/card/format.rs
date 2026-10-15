//! The Visitenkarte as data and as text (spec 051 FR-009, FR-011, contracts/contact-card.md): a
//! postcard struct signed by the vault identity, as `holzi:card:` and base64url of
//! `len(u16 BE) ‖ postcard ‖ signature`, at most 2,048 characters. A card carries no key and no
//! secret: the vault's public key, its devices with their network ids, the Nostr-Relays, a name and
//! the values the person chose.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::HolziError;
use crate::sync::signing::{self, Domain};

/// The start of a card's text.
pub const PREFIX: &str = "holzi:card:";
/// Most characters of a card's text.
pub const MAX_TEXT: usize = 2048;
/// Most Nostr-Relays on a card.
pub const MAX_RELAYS: usize = 10;
/// Most devices on a card.
pub const MAX_DEVICES: usize = 32;
/// Most bytes of a string on a card.
pub const MAX_STRING: usize = 1024;
/// The version this format has.
pub const VERSION: u8 = 1;

/// The signed content of a card. Field order is part of the format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactCard {
    pub version: u8,
    /// x-only public key of the vault identity.
    pub vault: [u8; 32],
    /// Unix seconds.
    pub issued_at: u64,
    /// Generation of the device list the devices come from.
    pub list_generation: u64,
    pub devices: Vec<CardDevice>,
    pub nostr_relays: Vec<String>,
    pub name: String,
    pub values: Vec<CardValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDevice {
    pub device_pubkey: [u8; 32],
    pub endpoint_id: [u8; 32],
    pub iroh_relay: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardValue {
    pub kind: String,
    pub label: String,
    pub value: String,
}

/// A card with its bytes as they travel (`len ‖ postcard ‖ signature`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedCard {
    pub card: ContactCard,
    pub bytes: Vec<u8>,
}

impl SignedCard {
    /// The card as text: link, clipboard and QR code.
    pub fn text(&self) -> String {
        format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(&self.bytes))
    }
}

fn invalid(reason: &str) -> HolziError {
    HolziError::CardInvalid {
        reason: reason.to_owned(),
    }
}

/// The limits every card keeps, on writing and on reading.
fn check_limits(card: &ContactCard) -> Result<(), HolziError> {
    if card.version != VERSION {
        return Err(invalid("unknown version"));
    }
    if card.devices.len() > MAX_DEVICES {
        return Err(invalid("too many devices"));
    }
    if card.nostr_relays.len() > MAX_RELAYS {
        return Err(invalid("too many relays"));
    }
    if !card
        .nostr_relays
        .iter()
        .all(|url| url.starts_with("wss://") || url.starts_with("ws://"))
    {
        return Err(invalid("a relay is no ws(s):// address"));
    }
    let strings = card
        .nostr_relays
        .iter()
        .chain(std::iter::once(&card.name))
        .chain(
            card.devices
                .iter()
                .filter_map(|device| device.iroh_relay.as_ref()),
        )
        .chain(
            card.values
                .iter()
                .flat_map(|value| [&value.kind, &value.label, &value.value]),
        );
    for text in strings {
        if text.len() > MAX_STRING {
            return Err(invalid("a text is too long"));
        }
    }
    Ok(())
}

fn encode(card: &ContactCard, secret: &[u8; 32]) -> Result<Vec<u8>, HolziError> {
    let payload = signing::canonical(card).map_err(|error| invalid(&error.to_string()))?;
    let signature = signing::sign(Domain::ContactCard, &payload, secret)
        .map_err(|error| invalid(&error.to_string()))?;
    let length = u16::try_from(payload.len()).map_err(|_| HolziError::CardTooLarge)?;
    let mut bytes = Vec::with_capacity(2 + payload.len() + 64);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&signature);
    Ok(bytes)
}

/// The length of the text of `bytes`.
fn text_len(bytes: &[u8]) -> usize {
    PREFIX.len() + (bytes.len() * 4).div_ceil(3)
}

/// Signs a card with the vault secret. Values that do not fit 2,048 characters are left out from
/// the end and returned; devices and relays are never left out.
pub fn sign(
    mut card: ContactCard,
    secret: &[u8; 32],
) -> Result<(SignedCard, Vec<CardValue>), HolziError> {
    check_limits(&card)?;
    let mut omitted = Vec::new();
    loop {
        let bytes = encode(&card, secret)?;
        if text_len(&bytes) <= MAX_TEXT {
            omitted.reverse();
            return Ok((SignedCard { card, bytes }, omitted));
        }
        match card.values.pop() {
            Some(value) => omitted.push(value),
            None => return Err(HolziError::CardTooLarge),
        }
    }
}

/// Reads a card from its text or its stored bytes and checks format, limits and signature
/// (FR-011). Whether it is the own card or an older one is the caller's check.
pub fn parse(text: &str) -> Result<SignedCard, HolziError> {
    let text = text.trim();
    let encoded = text
        .strip_prefix(PREFIX)
        .ok_or_else(|| invalid("not a holzi business card"))?;
    if text.len() > MAX_TEXT {
        return Err(invalid("too long"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| invalid("not base64url"))?;
    from_bytes(bytes)
}

/// Reads a card from the bytes stored in `haex_contacts_cards` or `haex_contacts_own_card`.
pub fn from_bytes(bytes: Vec<u8>) -> Result<SignedCard, HolziError> {
    let [high, low, rest @ ..] = bytes.as_slice() else {
        return Err(invalid("too short"));
    };
    let length = usize::from(u16::from_be_bytes([*high, *low]));
    if rest.len() != length + 64 {
        return Err(invalid("wrong length"));
    }
    let (payload, signature) = rest.split_at(length);
    let (card, remainder) =
        postcard::take_from_bytes::<ContactCard>(payload).map_err(|_| invalid("not a card"))?;
    if !remainder.is_empty() {
        return Err(invalid("trailing bytes"));
    }
    check_limits(&card)?;
    let signature: [u8; 64] = signature.try_into().map_err(|_| invalid("no signature"))?;
    signing::verify(Domain::ContactCard, payload, &signature, &card.vault)
        .map_err(|_| invalid("the signature does not verify"))?;
    Ok(SignedCard { card, bytes })
}

/// The card's QR code as SVG, error correction M.
pub fn qr_svg(text: &str) -> Result<String, HolziError> {
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M)
        .map_err(|_| HolziError::CardTooLarge)?;
    Ok(code
        .render::<qrcode::render::svg::Color<'_>>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .build())
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;

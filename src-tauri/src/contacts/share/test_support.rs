//! Helpers for the tests of reading shares in: shares of another vault, made by hand.

use super::envelope::{encode, ShareEnvelope, ShareValue};
use crate::contacts::card::format::{self, CardDevice, ContactCard, SignedCard};
use crate::sync::signing;

/// The secret of the vault that "sends" the shares of these tests.
pub(crate) const BEN: [u8; 32] = [7; 32];
/// A second vault.
pub(crate) const MARA: [u8; 32] = [8; 32];

pub(crate) fn vault_of(secret: &[u8; 32]) -> [u8; 32] {
    signing::xonly_public_key(secret).expect("a key")
}

/// A card of the vault of `secret`, with devices numbered `devices` and the Nostr-Relays given,
/// signed at `issued_at`.
pub(crate) fn card(
    secret: &[u8; 32],
    devices: &[u8],
    relays: &[&str],
    issued_at: u64,
) -> SignedCard {
    let card = ContactCard {
        version: format::VERSION,
        vault: vault_of(secret),
        issued_at,
        list_generation: 1,
        devices: devices
            .iter()
            .map(|n| CardDevice {
                device_pubkey: [*n; 32],
                endpoint_id: [n.wrapping_add(100); 32],
                iroh_relay: Some("https://relay.example.org/".to_owned()),
            })
            .collect(),
        nostr_relays: relays.iter().map(|relay| (*relay).to_owned()).collect(),
        name: "Ben Krüger".to_owned(),
        values: Vec::new(),
    };
    format::sign_with(card, secret, None).expect("sign").0
}

pub(crate) fn value(kind: &str, label: &str, text: &str) -> ShareValue {
    ShareValue {
        kind: kind.to_owned(),
        label: label.to_owned(),
        value: text.to_owned(),
    }
}

/// An envelope of "Ben Krüger" with the given values and card.
pub(crate) fn envelope(values: Vec<ShareValue>, card: Option<&SignedCard>) -> ShareEnvelope {
    let mut envelope = ShareEnvelope::new("Ben Krüger".to_owned());
    envelope.name_parts.given = Some("Ben".to_owned());
    envelope.name_parts.family = Some("Krüger".to_owned());
    envelope.values = values;
    envelope.card = card.map(|card| card.bytes.clone());
    envelope
}

pub(crate) fn text(envelope: &ShareEnvelope) -> String {
    encode(envelope).expect("encodes")
}

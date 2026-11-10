//! The link code and what derives from it (spec 024, FR-024, research R11).
//!
//! 16 random bytes, shown as 26 Base32 characters in groups of four and as a
//! QR code (of the link text, see `text`), valid once and for ten minutes. Three independent keys derive
//! from it with HKDF-SHA256: the rendezvous key pair the new installation
//! announces itself to, the key both sides prove the code with, and, mixed
//! with both nonces, the session secret a resumed link authenticates with.

use std::time::Duration;

use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use nostr::key::{Keys, PublicKey, SecretKey};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::sync::keys::random_bytes;
use crate::sync::signing::lp;

/// Bytes of randomness in a code: 128 bits.
pub const CODE_BYTES: usize = 16;
/// Characters of the Base32 form (128 bits do not fill 26 × 5).
const CODE_CHARS: usize = 26;
/// How long a shown code stays usable.
pub const CODE_LIFETIME: Duration = Duration::from_secs(10 * 60);

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Which side of the link a proof belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The main device that shows the code.
    Host,
    /// The new installation that enters it.
    Joiner,
}

impl Role {
    /// The byte tag that distinguishes the host's proof from the joiner's.
    fn tag(self) -> &'static [u8] {
        match self {
            Role::Host => b"H",
            Role::Joiner => b"N",
        }
    }
}

/// Why text is not a link code.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodeError {
    #[error("a link code has {CODE_CHARS} characters, without spaces and dashes")]
    Length,
    #[error("a link code only has the letters A to Z and the digits 2 to 7")]
    Character,
}

/// A link code. Never printed by `Debug`, never logged.
#[derive(Clone)]
pub struct LinkCode(Zeroizing<[u8; CODE_BYTES]>);

impl std::fmt::Debug for LinkCode {
    /// Formats a placeholder without exposing the code's secret bytes.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LinkCode(..)")
    }
}

impl LinkCode {
    /// A fresh random code.
    pub fn generate() -> Self {
        Self(random_bytes::<CODE_BYTES>())
    }

    /// Reads a code as typed or pasted: case, spaces and dashes do not
    /// matter.
    pub fn parse(text: &str) -> Result<Self, CodeError> {
        let chars: Vec<u8> = text
            .bytes()
            .filter(|b| !b.is_ascii_whitespace() && *b != b'-')
            .map(|b| b.to_ascii_uppercase())
            .collect();
        if chars.len() != CODE_CHARS {
            return Err(CodeError::Length);
        }
        let mut bits = 0u32;
        let mut have = 0u32;
        let mut bytes = Zeroizing::new([0u8; CODE_BYTES]);
        let mut at = 0;
        for c in chars {
            let value = ALPHABET
                .iter()
                .position(|a| *a == c)
                .ok_or(CodeError::Character)? as u32;
            bits = (bits << 5) | value;
            have += 5;
            if have >= 8 {
                have -= 8;
                if at < CODE_BYTES {
                    bytes[at] = (bits >> have) as u8;
                    at += 1;
                }
                bits &= (1 << have) - 1;
            }
        }
        // 26 × 5 = 130 bits: the two left over must be zero, or this is not
        // a code this device showed (a typo the length check missed).
        if bits != 0 {
            return Err(CodeError::Character);
        }
        Ok(Self(bytes))
    }

    /// The code as shown: uppercase Base32 in groups of four.
    pub fn display(&self) -> String {
        let mut out = String::with_capacity(CODE_CHARS + CODE_CHARS / 4);
        let mut bits = 0u32;
        let mut have = 0u32;
        let mut count = 0;
        let mut push = |value: u32, out: &mut String| {
            if count > 0 && count % 4 == 0 {
                out.push('-');
            }
            out.push(ALPHABET[value as usize] as char);
            count += 1;
        };
        for byte in self.0.iter() {
            bits = (bits << 8) | u32::from(*byte);
            have += 8;
            while have >= 5 {
                have -= 5;
                push((bits >> have) & 31, &mut out);
                bits &= (1 << have) - 1;
            }
        }
        if have > 0 {
            push((bits << (5 - have)) & 31, &mut out);
        }
        out
    }

    /// Derives 32 bytes with HKDF-SHA256 using the given salt and purpose label.
    fn expand(&self, salt: Option<&[u8]>, info: &[u8]) -> Zeroizing<[u8; 32]> {
        let mut okm = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(salt, self.0.as_slice())
            .expand(info, okm.as_mut_slice())
            .expect("32 bytes are a valid HKDF-SHA256 output length");
        okm
    }

    /// The key pair the joiner's meeting is sent to (contracts/
    /// nostr-events.md): the host subscribes to it while the code is valid.
    pub fn rendezvous_keys(&self) -> Result<(SecretKey, PublicKey), CodeError> {
        let okm = self.expand(None, b"holzi/link/rendezvous/v1");
        let secret = SecretKey::from_slice(okm.as_slice()).map_err(|_| CodeError::Character)?;
        let public = Keys::new(secret.clone()).public_key();
        Ok((secret, public))
    }

    /// The proof of knowing the code over `transcript`.
    pub fn proof(&self, role: Role, transcript: &Transcript) -> [u8; 32] {
        let mut mac = self.proof_mac();
        mac.update(&transcript.bytes(role));
        mac.finalize().into_bytes().into()
    }

    /// Whether `tag` is `role`'s proof over `transcript`, in constant time.
    pub fn verify_proof(&self, role: Role, transcript: &Transcript, tag: &[u8]) -> bool {
        let mut mac = self.proof_mac();
        mac.update(&transcript.bytes(role));
        mac.verify_slice(tag).is_ok()
    }

    /// Initializes HMAC-SHA256 with the proof key derived from this code.
    fn proof_mac(&self) -> Hmac<Sha256> {
        let key = self.expand(None, b"holzi/link/proof/v1");
        Hmac::<Sha256>::new_from_slice(key.as_slice()).expect("HMAC takes any key length")
    }

    /// The secret both sides keep after the code is used, for resuming a
    /// link without the code (contracts/sync-protocol.md).
    pub fn session_secret(&self, nonce_h: &[u8; 32], nonce_n: &[u8; 32]) -> Zeroizing<[u8; 32]> {
        self.expand(
            Some(&[nonce_h.as_slice(), nonce_n.as_slice()].concat()),
            b"holzi/link/session/v1",
        )
    }
}

/// What both sides sign into their proof: nonces, endpoints and device keys
/// of both, so a proof only holds for this very pair of connections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub nonce_h: [u8; 32],
    pub nonce_n: [u8; 32],
    pub endpoint_h: [u8; 32],
    pub endpoint_n: [u8; 32],
    pub device_h: [u8; 32],
    pub device_n: [u8; 32],
}

impl Transcript {
    /// Encodes the protocol label, length-prefixed fields and role for the proof.
    fn bytes(&self, role: Role) -> Vec<u8> {
        [
            b"holzi-link/v1".to_vec(),
            lp(&self.nonce_h),
            lp(&self.nonce_n),
            lp(&self.endpoint_h),
            lp(&self.endpoint_n),
            lp(&self.device_h),
            lp(&self.device_n),
            role.tag().to_vec(),
        ]
        .concat()
    }
}

/// The MAC a resumed link authenticates with: over the link id and the
/// state the resuming side claims.
pub fn resume_mac(secret: &[u8; 32], link_id: &[u8; 32], state: &str) -> [u8; 32] {
    let mut mac = resume_hmac(secret);
    mac.update(&resume_bytes(link_id, state));
    mac.finalize().into_bytes().into()
}

/// Whether `tag` is [`resume_mac`], in constant time.
pub fn verify_resume_mac(secret: &[u8; 32], link_id: &[u8; 32], state: &str, tag: &[u8]) -> bool {
    let mut mac = resume_hmac(secret);
    mac.update(&resume_bytes(link_id, state));
    mac.verify_slice(tag).is_ok()
}

/// Initializes HMAC-SHA256 with the shared session secret for a resume proof.
fn resume_hmac(secret: &[u8; 32]) -> Hmac<Sha256> {
    Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes any key length")
}

/// Encodes the resume protocol label and length-prefixed link id and state.
fn resume_bytes(link_id: &[u8; 32], state: &str) -> Vec<u8> {
    [
        b"holzi-link-resume/v1".to_vec(),
        lp(link_id),
        lp(state.as_bytes()),
    ]
    .concat()
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;

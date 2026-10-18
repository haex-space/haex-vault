//! Turning a passkey of a source into a [`PasskeyInput`] (spec 034, research R12 point 8). The
//! sources deliver the private key only (PKCS8); the table also wants the public key (SPKI), which
//! is derived here for ES256, EdDSA and RS256. A key that cannot be read or derived does not stop
//! the import: the passkey is stored with what is known and the problem goes to the report.

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use zeroize::Zeroizing;

use super::Problem;
use crate::passwords::model::AttentionKind;
use crate::passwords::passkeys::{derive_public_key, DeriveError, PasskeyInput};

const ES256: i64 = -7;
const EDDSA: i64 = -8;
const RS256: i64 = -257;

/// A passkey as a source gives it. `credential_id` is already the standard Base64 of its bytes.
pub struct RawPasskey {
    pub credential_id: String,
    pub relying_party_id: String,
    pub relying_party_name: Option<String>,
    pub user_name: Option<String>,
    pub user_display_name: Option<String>,
    pub user_handle: String,
    /// Base64 (standard or URL-safe) of the PKCS8 key, as found; a PEM block is stripped by the caller.
    pub private_key: Zeroizing<String>,
    /// The COSE algorithm if the source names it; `None` makes the importer try all three.
    pub algorithm: Option<i64>,
    pub sign_count: i64,
    pub is_discoverable: bool,
    pub created_at: Option<String>,
}

/// Decodes Base64 in any of its four common forms.
pub fn decode_base64_any(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    STANDARD
        .decode(text)
        .or_else(|_| STANDARD_NO_PAD.decode(text))
        .or_else(|_| URL_SAFE.decode(text))
        .or_else(|_| URL_SAFE_NO_PAD.decode(text))
        .ok()
}

/// The body of a PEM block as Base64 text (header and footer lines and line breaks removed).
pub fn strip_pem(pem: &str) -> String {
    pem.lines()
        .filter(|line| !line.trim_start().starts_with("-----"))
        .map(str::trim)
        .collect()
}

/// Builds the passkey and pushes a [`Problem`] for what could not be carried over.
pub fn build(raw: RawPasskey, problems: &mut Vec<Problem>) -> PasskeyInput {
    let place = format!("passkey {}", raw.relying_party_id);
    let mut algorithm = raw.algorithm.unwrap_or(0);
    let mut public_key = String::new();
    let mut private_key = raw.private_key.clone();
    match decode_base64_any(&raw.private_key) {
        None => problems.push(Problem::field(AttentionKind::PasskeyKeyUnreadable, &place)),
        Some(der) => {
            let der = Zeroizing::new(der);
            // The key is stored the way haex-vault 2.x (Legacy) stores it: standard Base64 of the PKCS8 DER.
            private_key = Zeroizing::new(STANDARD.encode(&*der));
            let candidates: Vec<i64> = match raw.algorithm {
                Some(a) => vec![a],
                None => vec![ES256, EDDSA, RS256],
            };
            let mut outcome = Err(DeriveError::UnreadableKey);
            for candidate in candidates {
                match derive_public_key(candidate, &der) {
                    Ok(spki) => {
                        algorithm = candidate;
                        outcome = Ok(spki);
                        break;
                    }
                    Err(DeriveError::UnsupportedAlgorithm) => {
                        outcome = Err(DeriveError::UnsupportedAlgorithm);
                    }
                    Err(DeriveError::UnreadableKey) => {
                        if !matches!(outcome, Err(DeriveError::UnsupportedAlgorithm)) {
                            outcome = Err(DeriveError::UnreadableKey);
                        }
                    }
                }
            }
            match outcome {
                Ok(spki) => public_key = STANDARD.encode(spki),
                Err(DeriveError::UnsupportedAlgorithm) => problems.push(Problem::field(
                    AttentionKind::PasskeyPublicKeyMissing,
                    &place,
                )),
                Err(DeriveError::UnreadableKey) => {
                    problems.push(Problem::field(AttentionKind::PasskeyKeyUnreadable, &place))
                }
            }
        }
    }
    PasskeyInput {
        item_id: None,
        credential_id: raw.credential_id,
        relying_party_id: raw.relying_party_id,
        relying_party_name: raw.relying_party_name,
        user_name: raw.user_name,
        user_display_name: raw.user_display_name,
        user_handle: raw.user_handle,
        private_key,
        public_key,
        algorithm,
        sign_count: raw.sign_count,
        is_discoverable: raw.is_discoverable,
        icon: None,
        color: None,
        nickname: None,
        created_at: raw.created_at,
        last_used_at: None,
    }
}

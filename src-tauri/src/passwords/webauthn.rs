//! The WebAuthn building blocks of the passkey service (spec 036, research R8, R9,
//! `contracts/passkey-service.md`): the origin check, new key pairs, COSE keys, authenticator data,
//! the attestation object, the client data and the signature. Pure: no database, no caller. No
//! function logs, prints or returns a private key except [`KeyPair::pkcs8`], whose `Debug` is
//! redacted; every error is a bare kind without key material.

use std::fmt;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ciborium::Value;
use pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey};
use sha2::{Digest, Sha256};
use url::{Host, Url};
use zeroize::Zeroizing;

use super::passkeys::{EDDSA, ES256};

/// User present. Not set: holzi has no trusted, ceremony-bound proof that a person is there (R8).
pub const FLAG_UP: u8 = 0x01;
/// User verified. Never set: no person confirms the request (R8).
pub const FLAG_UV: u8 = 0x04;
/// Backup eligible: a synced passkey.
pub const FLAG_BE: u8 = 0x08;
/// Backed up.
pub const FLAG_BS: u8 = 0x10;
/// Attested credential data follows (create only).
pub const FLAG_AT: u8 = 0x40;

/// The longest user handle WebAuthn allows, in bytes after decoding.
pub const MAX_USER_HANDLE_BYTES: usize = 64;

/// Why a building block refused. Names a field at most, never a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebauthnError {
    /// A field of the request is missing or malformed.
    InvalidInput(&'static str),
    /// The algorithm is neither ES256 nor EdDSA.
    UnsupportedAlgorithm,
    /// A stored key does not decode for its algorithm.
    UnreadableKey,
}

/// The ASCII (IDNA) form of a relying party id, or `None` when it is no domain: empty, with a
/// port or a path, an IP address, or a public suffix (`com`, `co.uk`; `localhost` is allowed).
fn relying_party(rp_id: &str) -> Option<String> {
    if rp_id.is_empty() || rp_id.contains([':', '/', '@', '?', '#']) {
        return None;
    }
    let Ok(Host::Domain(domain)) = Host::parse(rp_id) else {
        return None;
    };
    let domain = domain.to_ascii_lowercase();
    if domain == "localhost" {
        return Some(domain);
    }
    // A name that is its own public suffix belongs to nobody; signing for it would sign for every
    // site below it (R9).
    (!super::site::is_public_suffix(&domain)).then_some(domain)
}

/// The origin in its serialised ASCII form and the relying party id in its ASCII form, when the
/// origin belongs to that relying party (R9): `https` (`http` only for `localhost`), nothing but
/// scheme, host and port, the host is the relying party or below it, and the relying party is a
/// domain that is not a public suffix.
pub fn checked_origin(origin: &str, rp_id: &str) -> Option<(String, String)> {
    let rp = relying_party(rp_id)?;
    let url = Url::parse(origin).ok()?;
    let Some(Host::Domain(host)) = url.host() else {
        return None;
    };
    let host = host.to_ascii_lowercase();
    let local = host == "localhost" || host.ends_with(".localhost");
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        _ => return None,
    }
    // `Url` gives an origin without a path the path "/".
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    if host != rp && !host.ends_with(&format!(".{rp}")) {
        return None;
    }
    Some((url.origin().ascii_serialization(), rp))
}

/// Whether `origin` may use passkeys of `rp_id` (see [`checked_origin`]).
pub fn origin_matches(origin: &str, rp_id: &str) -> bool {
    checked_origin(origin, rp_id).is_some()
}

/// The bytes of a Base64URL field (padding tolerated).
pub fn decode_b64url(field: &'static str, text: &str) -> Result<Vec<u8>, WebauthnError> {
    URL_SAFE_NO_PAD
        .decode(text.trim_end_matches('='))
        .map_err(|_| WebauthnError::InvalidInput(field))
}

/// The user handle: Base64URL, 1 to [`MAX_USER_HANDLE_BYTES`] bytes.
pub fn decode_user_handle(text: &str) -> Result<Vec<u8>, WebauthnError> {
    let bytes = decode_b64url("userHandle", text)?;
    if bytes.is_empty() || bytes.len() > MAX_USER_HANDLE_BYTES {
        return Err(WebauthnError::InvalidInput("userHandle"));
    }
    Ok(bytes)
}

/// The algorithm for a new passkey: ES256 if the relying party takes it, else EdDSA.
pub fn choose_algorithm(params: &[i64]) -> Option<i64> {
    [ES256, EDDSA]
        .into_iter()
        .find(|algorithm| params.contains(algorithm))
}

/// A new key pair: the private key as PKCS8 DER, the public key as SPKI DER.
pub struct KeyPair {
    pub algorithm: i64,
    pub pkcs8: Zeroizing<Vec<u8>>,
    pub spki: Vec<u8>,
}

impl fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyPair")
            .field("algorithm", &self.algorithm)
            .field("pkcs8", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// Any decoding or encoding error of a key is just "unreadable": the error value is dropped so
/// that no key material can travel with it.
fn unreadable<E>(_: E) -> WebauthnError {
    WebauthnError::UnreadableKey
}

fn random_32() -> Result<Zeroizing<[u8; 32]>, WebauthnError> {
    let mut bytes = Zeroizing::new([0u8; 32]);
    // A failing system random source is no input error, but nothing can be created without it.
    getrandom::fill(bytes.as_mut()).map_err(unreadable)?;
    Ok(bytes)
}

/// 32 random bytes, for a credential id.
pub fn random_credential_id() -> Result<Vec<u8>, WebauthnError> {
    Ok(random_32()?.to_vec())
}

/// A new key pair for ES256 (P-256) or EdDSA (Ed25519).
pub fn generate_key(algorithm: i64) -> Result<KeyPair, WebauthnError> {
    match algorithm {
        ES256 => {
            // A random scalar is out of range with a chance of about 2^-128; draw again then.
            let secret = loop {
                if let Ok(secret) = p256::SecretKey::from_slice(random_32()?.as_ref()) {
                    break secret;
                }
            };
            let pkcs8 = secret.to_pkcs8_der().map_err(unreadable)?;
            let spki = secret
                .public_key()
                .to_public_key_der()
                .map_err(unreadable)?;
            Ok(KeyPair {
                algorithm,
                pkcs8: Zeroizing::new(pkcs8.as_bytes().to_vec()),
                spki: spki.as_bytes().to_vec(),
            })
        }
        EDDSA => {
            let signing = ed25519_dalek::SigningKey::from_bytes(&*random_32()?);
            let pkcs8 = signing.to_pkcs8_der().map_err(unreadable)?;
            let spki = signing
                .verifying_key()
                .to_public_key_der()
                .map_err(unreadable)?;
            Ok(KeyPair {
                algorithm,
                pkcs8: Zeroizing::new(pkcs8.as_bytes().to_vec()),
                spki: spki.as_bytes().to_vec(),
            })
        }
        _ => Err(WebauthnError::UnsupportedAlgorithm),
    }
}

/// A decoded COSE public key, for checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoseKey {
    /// P-256 with ES256: the coordinates.
    Ec2 { x: Vec<u8>, y: Vec<u8> },
    /// Ed25519 with EdDSA: the public key.
    Okp { x: Vec<u8> },
}

fn int(value: i64) -> Value {
    Value::Integer(value.into())
}

/// The COSE key of a public key (SPKI DER), in CTAP2 canonical order: `1` (kty), `3` (alg),
/// `-1` (crv), `-2` (x), `-3` (y).
pub fn cose_public_key(algorithm: i64, spki: &[u8]) -> Result<Vec<u8>, WebauthnError> {
    let map = match algorithm {
        ES256 => {
            use p256::elliptic_curve::sec1::ToSec1Point as _;
            let public = p256::PublicKey::from_public_key_der(spki).map_err(unreadable)?;
            let point = public.to_sec1_point(false);
            let bytes = point.as_bytes();
            // Uncompressed: 0x04 ‖ x ‖ y.
            if bytes.len() != 65 {
                return Err(WebauthnError::UnreadableKey);
            }
            vec![
                (int(1), int(2)),
                (int(3), int(ES256)),
                (int(-1), int(1)),
                (int(-2), Value::Bytes(bytes[1..33].to_vec())),
                (int(-3), Value::Bytes(bytes[33..].to_vec())),
            ]
        }
        EDDSA => {
            let public =
                ed25519_dalek::VerifyingKey::from_public_key_der(spki).map_err(unreadable)?;
            vec![
                (int(1), int(1)),
                (int(3), int(EDDSA)),
                (int(-1), int(6)),
                (int(-2), Value::Bytes(public.to_bytes().to_vec())),
            ]
        }
        _ => return Err(WebauthnError::UnsupportedAlgorithm),
    };
    let mut out = Vec::new();
    ciborium::into_writer(&Value::Map(map), &mut out).map_err(unreadable)?;
    Ok(out)
}

/// Reads a COSE key written by [`cose_public_key`].
pub fn decode_cose_key(bytes: &[u8]) -> Result<CoseKey, WebauthnError> {
    let unreadable = WebauthnError::UnreadableKey;
    let Value::Map(entries) = ciborium::from_reader::<Value, _>(bytes).map_err(|_| unreadable)?
    else {
        return Err(unreadable);
    };
    let get = |key: i64| {
        entries
            .iter()
            .find(|(k, _)| k.as_integer() == Some(key.into()))
            .map(|(_, v)| v)
    };
    let integer = |key: i64| get(key).and_then(Value::as_integer).map(i128::from);
    let bytes = |key: i64| get(key).and_then(Value::as_bytes).cloned();
    match (integer(1), integer(3), integer(-1)) {
        (Some(2), Some(-7), Some(1)) => Ok(CoseKey::Ec2 {
            x: bytes(-2).ok_or(unreadable)?,
            y: bytes(-3).ok_or(unreadable)?,
        }),
        (Some(1), Some(-8), Some(6)) => Ok(CoseKey::Okp {
            x: bytes(-2).ok_or(unreadable)?,
        }),
        _ => Err(unreadable),
    }
}

/// The SHA-256 of the relying party id in its ASCII form (as given when it is no domain; the
/// service checks it before).
fn rp_id_hash(rp_id: &str) -> [u8; 32] {
    let ascii = match Host::parse(rp_id) {
        Ok(Host::Domain(domain)) => domain.to_ascii_lowercase(),
        _ => rp_id.to_owned(),
    };
    Sha256::digest(ascii.as_bytes()).into()
}

/// Authenticator data for a create (flags `0x58`: BE, BS, AT): the relying party hash, the flags,
/// the counter 0, an AAGUID of zeros, the credential id with its length and the COSE key.
pub fn create_auth_data(rp_id: &str, credential_id: &[u8], cose_key: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(55 + credential_id.len() + cose_key.len());
    data.extend_from_slice(&rp_id_hash(rp_id));
    data.push(FLAG_BE | FLAG_BS | FLAG_AT);
    // ponytail: the counter is always 0 (research R5); a synced passkey has no global order.
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&[0u8; 16]);
    // A credential id is 32 bytes here; WebAuthn allows up to 1023.
    let length = u16::try_from(credential_id.len()).unwrap_or(u16::MAX);
    data.extend_from_slice(&length.to_be_bytes());
    data.extend_from_slice(credential_id);
    data.extend_from_slice(cose_key);
    data
}

/// Authenticator data for a get (flags `0x18`: BE, BS): the relying party hash, the flags and the
/// counter 0.
pub fn get_auth_data(rp_id: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(37);
    data.extend_from_slice(&rp_id_hash(rp_id));
    // ponytail: no UP and no UV until a trusted, ceremony-bound presence provider exists (R8).
    data.push(FLAG_BE | FLAG_BS);
    data.extend_from_slice(&0u32.to_be_bytes());
    data
}

/// The attestation object `{fmt: "none", attStmt: {}, authData}`.
pub fn attestation_object(auth_data: &[u8]) -> Vec<u8> {
    // ponytail: attestation "none"; holzi has no attestation key a relying party could trust.
    let map = Value::Map(vec![
        (Value::Text("fmt".into()), Value::Text("none".into())),
        (Value::Text("attStmt".into()), Value::Map(vec![])),
        (
            Value::Text("authData".into()),
            Value::Bytes(auth_data.to_vec()),
        ),
    ]);
    let mut out = Vec::new();
    // Writing into a Vec does not fail.
    let _ = ciborium::into_writer(&map, &mut out);
    out
}

/// Which ceremony the client data belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ceremony {
    Create,
    Get,
}

/// The client data JSON, built by the service so that origin and challenge cannot drift apart:
/// `{"type":…,"challenge":…,"origin":…,"crossOrigin":false}` in this order.
pub fn client_data_json(ceremony: Ceremony, challenge: &[u8], origin: &str) -> String {
    let kind = match ceremony {
        Ceremony::Create => "webauthn.create",
        Ceremony::Get => "webauthn.get",
    };
    // Serialising a string does not fail.
    let origin = serde_json::to_string(origin).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"{{"type":"{kind}","challenge":"{}","origin":{origin},"crossOrigin":false}}"#,
        URL_SAFE_NO_PAD.encode(challenge)
    )
}

/// The signature over `auth_data ‖ SHA-256(client_data_json)`: ES256 as DER ECDSA, EdDSA as 64
/// bytes.
pub fn sign(
    algorithm: i64,
    pkcs8: &[u8],
    auth_data: &[u8],
    client_data_json: &[u8],
) -> Result<Vec<u8>, WebauthnError> {
    let mut message = auth_data.to_vec();
    message.extend_from_slice(&Sha256::digest(client_data_json));
    match algorithm {
        ES256 => {
            use p256::ecdsa::signature::Signer as _;
            let secret =
                p256::SecretKey::from_pkcs8_der(pkcs8).map_err(|_| WebauthnError::UnreadableKey)?;
            let key = p256::ecdsa::SigningKey::from(secret);
            let signature: p256::ecdsa::Signature = key.sign(&message);
            Ok(signature.to_der().as_bytes().to_vec())
        }
        EDDSA => {
            use ed25519_dalek::Signer as _;
            let key = ed25519_dalek::SigningKey::from_pkcs8_der(pkcs8)
                .map_err(|_| WebauthnError::UnreadableKey)?;
            Ok(key.sign(&message).to_bytes().to_vec())
        }
        _ => Err(WebauthnError::UnsupportedAlgorithm),
    }
}

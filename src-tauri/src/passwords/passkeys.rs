//! Passkeys as data (spec 034, FR-004, research R12): list, rename and delete, the insert for the
//! import, and the derivation of the public key from the private one. The window never creates a
//! passkey and nothing here signs; a key is never listed and never printed.
//!
//! Encodings follow haex-vault 2.x (Legacy) (research R12, spike T003): `credential_id` is standard Base64 of
//! the credential's bytes, `private_key` standard Base64 of the PKCS8 DER, `public_key` standard
//! Base64 of the SPKI DER, `algorithm` a COSE number (-7 ES256, -8 EdDSA, -257 RS256).

use std::fmt;

use base64::Engine;
use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use pkcs8::der::asn1::{AnyRef, BitStringRef};
use pkcs8::der::{Decode, Encode};
use pkcs8::{DecodePrivateKey, EncodePublicKey, PrivateKeyInfoRef};
use zeroize::Zeroizing;

use super::ids::passkey_id;
use super::model::PasskeyView;
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

pub const ES256: i64 = -7;
pub const EDDSA: i64 = -8;
pub const RS256: i64 = -257;

/// The passkeys of an entry without their keys: its own, then those of other entries it shows by
/// a link (spec 036, research R6).
pub fn list_for_item(q: &mut impl Query, item_id: &str) -> Result<Vec<PasskeyView>> {
    let mut own = q.query_map(
        "SELECT id, relying_party_id, relying_party_name, user_name, nickname, algorithm, \
                created_at, last_used_at, is_discoverable, sign_count \
         FROM haex_passwords_passkeys WHERE item_id = ?1 ORDER BY rowid",
        params![item_id],
        |r| {
            Ok(PasskeyView {
                id: r.get(0)?,
                relying_party_id: r.get(1)?,
                relying_party_name: r.get(2)?,
                user_name: r.get(3)?,
                nickname: r.get(4)?,
                algorithm: r.get(5)?,
                created_at: r.get(6)?,
                last_used_at: r.get(7)?,
                item_id: Some(item_id.to_owned()),
                is_discoverable: r.get::<_, i64>(8)? != 0,
                sign_count: r.get(9)?,
                linked_from: None,
            })
        },
    )?;
    own.extend(super::passkey_links::linked_views(q, item_id)?);
    Ok(own)
}

/// Sets the nickname of a passkey (`None` clears it); nothing else changes.
pub fn rename(
    tx: &mut CrdtTransaction<'_>,
    passkey_id: &str,
    nickname: Option<&str>,
) -> Result<()> {
    let changed = tx.execute(
        "UPDATE haex_passwords_passkeys SET nickname = ?1 WHERE id = ?2",
        params![nickname, passkey_id],
    )?;
    if changed == 0 {
        return Err(HolziError::PasswordsNotFound);
    }
    Ok(())
}

/// Deletes one passkey and, first, its links to other entries; the other passkeys stay.
pub fn delete(tx: &mut CrdtTransaction<'_>, passkey_id: &str) -> Result<()> {
    super::passkey_links::delete_for_passkey(tx, passkey_id)?;
    let changed = tx.execute(
        "DELETE FROM haex_passwords_passkeys WHERE id = ?1",
        params![passkey_id],
    )?;
    if changed == 0 {
        return Err(HolziError::PasswordsNotFound);
    }
    Ok(())
}

/// A passkey to insert, as the import hands it over. `Debug` prints no key.
#[derive(Clone)]
pub struct PasskeyInput {
    pub item_id: Option<String>,
    pub credential_id: String,
    pub relying_party_id: String,
    pub relying_party_name: Option<String>,
    pub user_name: Option<String>,
    pub user_display_name: Option<String>,
    pub user_handle: String,
    pub private_key: Zeroizing<String>,
    /// Empty when it could not be derived (the passkey is stored all the same).
    pub public_key: String,
    pub algorithm: i64,
    pub sign_count: i64,
    pub is_discoverable: bool,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub nickname: Option<String>,
    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
}

impl fmt::Debug for PasskeyInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasskeyInput")
            .field("item_id", &self.item_id)
            .field("relying_party_id", &self.relying_party_id)
            .field("algorithm", &self.algorithm)
            .field("private_key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// What [`insert`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    /// A new row with this id (`UUIDv5(NS_PASSKEY, credential_id)`).
    Created(String),
    /// A passkey with this credential id is already there; nothing was written.
    Duplicate,
}

/// Inserts a passkey unless its credential id is already known (check-then-write; there is no
/// UNIQUE constraint, the derived id is the key).
pub fn insert(tx: &mut CrdtTransaction<'_>, input: &PasskeyInput) -> Result<InsertOutcome> {
    let id = passkey_id(&input.credential_id).to_string();
    let exists = tx
        .query_row(
            "SELECT COUNT(*) FROM haex_passwords_passkeys WHERE id = ?1 OR credential_id = ?2",
            params![id, input.credential_id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0);
    if exists > 0 {
        return Ok(InsertOutcome::Duplicate);
    }
    let created = input.created_at.clone().unwrap_or_else(super::clock::now);
    tx.execute(
        "INSERT INTO haex_passwords_passkeys \
         (id, item_id, credential_id, relying_party_id, relying_party_name, user_name, \
          user_display_name, user_handle, private_key, public_key, algorithm, sign_count, \
          is_discoverable, icon, color, nickname, created_at, last_used_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            id,
            input.item_id,
            input.credential_id,
            input.relying_party_id,
            input.relying_party_name,
            input.user_name,
            input.user_display_name,
            input.user_handle,
            input.private_key.as_str(),
            input.public_key,
            input.algorithm,
            input.sign_count,
            i64::from(input.is_discoverable),
            input.icon,
            input.color,
            input.nickname,
            created,
            input.last_used_at,
        ],
    )?;
    Ok(InsertOutcome::Created(id))
}

/// Why a public key could not be derived. It names no key material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeriveError {
    /// An algorithm other than ES256, EdDSA and RS256.
    UnsupportedAlgorithm,
    /// The bytes are not a PKCS8 key of that algorithm.
    UnreadableKey,
}

/// Any decoding or encoding error of a key is just "unreadable": the error value is dropped so
/// that no key material can travel with it.
fn unreadable<E>(_: E) -> DeriveError {
    DeriveError::UnreadableKey
}

/// The SPKI DER of the public key that belongs to a PKCS8 DER private key: ES256 (-7) over `p256`,
/// EdDSA (-8) over `ed25519-dalek`, RS256 (-257) from the modulus and public exponent of the RSA
/// key.
pub fn derive_public_key(
    cose_algorithm: i64,
    pkcs8_der: &[u8],
) -> std::result::Result<Vec<u8>, DeriveError> {
    match cose_algorithm {
        ES256 => {
            let secret = p256::SecretKey::from_pkcs8_der(pkcs8_der).map_err(unreadable)?;
            Ok(secret
                .public_key()
                .to_public_key_der()
                .map_err(unreadable)?
                .as_bytes()
                .to_vec())
        }
        EDDSA => {
            let signing =
                ed25519_dalek::SigningKey::from_pkcs8_der(pkcs8_der).map_err(unreadable)?;
            Ok(signing
                .verifying_key()
                .to_public_key_der()
                .map_err(unreadable)?
                .as_bytes()
                .to_vec())
        }
        RS256 => {
            let info = PrivateKeyInfoRef::try_from(pkcs8_der).map_err(unreadable)?;
            if info.algorithm.oid != pkcs1::ALGORITHM_OID {
                return Err(DeriveError::UnreadableKey);
            }
            let private =
                pkcs1::RsaPrivateKey::from_der(info.private_key.as_bytes()).map_err(unreadable)?;
            let public = pkcs1::RsaPublicKey {
                modulus: private.modulus,
                public_exponent: private.public_exponent,
            }
            .to_der()
            .map_err(unreadable)?;
            spki::SubjectPublicKeyInfoRef {
                algorithm: spki::AlgorithmIdentifierRef {
                    oid: pkcs1::ALGORITHM_OID,
                    parameters: Some(AnyRef::NULL),
                },
                subject_public_key: BitStringRef::from_bytes(&public).map_err(unreadable)?,
            }
            .to_der()
            .map_err(unreadable)
        }
        _ => Err(DeriveError::UnsupportedAlgorithm),
    }
}

/// [`derive_public_key`] for a private key as text (standard or URL-safe Base64, with or without
/// padding) and the public key as standard Base64, the form the table stores.
pub fn derive_public_key_base64(
    cose_algorithm: i64,
    private_key: &str,
) -> std::result::Result<String, DeriveError> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    let text = private_key.trim();
    let der = Zeroizing::new(
        STANDARD
            .decode(text)
            .or_else(|_| STANDARD_NO_PAD.decode(text))
            .or_else(|_| URL_SAFE.decode(text))
            .or_else(|_| URL_SAFE_NO_PAD.decode(text))
            .map_err(|_| DeriveError::UnreadableKey)?,
    );
    derive_public_key(cose_algorithm, &der).map(|public| STANDARD.encode(public))
}

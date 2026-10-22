//! The passkey service against the database (spec 036, US5, FR-023 to FR-036, research R5, R6,
//! R8, `contracts/passkey-service.md`): create, confirm and list for a caller with its grants. The
//! WebAuthn bytes come from the pure [`super::webauthn`]; here are the access rules, the candidates
//! and the rows. A private key leaves the database only into [`webauthn::sign`].
//!
//! A passkey is reachable through its own entry or, by a link, through a target entry; through a
//! link only when the target **and** the passkey's own entry are readable (FR-047). Entries in the
//! trash and passkeys without an entry are never candidates. One passkey reachable on several ways
//! is one candidate.

use std::collections::HashMap;

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use url::Host;
use zeroize::Zeroizing;

use super::access::{authorize_list, authorize_read, authorize_update, ItemState, ListView};
use super::items::item_state;
use super::model_passkeys::{
    PasskeyAssertion, PasskeyConfirmRequest, PasskeyCreateRequest, PasskeyCreated, PasskeyHeader,
    PasskeyListRequest, PasskeyTarget,
};
use super::passkeys::{self, InsertOutcome, PasskeyInput};
use super::passkeys_new_entry;
use super::references_db::Reader;
use super::webauthn::{self, Ceremony, WebauthnError};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

impl From<WebauthnError> for HolziError {
    fn from(error: WebauthnError) -> Self {
        match error {
            WebauthnError::InvalidInput(field) => HolziError::InvalidInput {
                reason: field.into(),
            },
            WebauthnError::UnsupportedAlgorithm | WebauthnError::UnreadableKey => {
                HolziError::PasswordsPasskeyUnsupportedAlgorithm
            }
        }
    }
}

fn required<'a>(field: &'static str, value: &'a str) -> Result<&'a str> {
    if value.trim().is_empty() {
        return Err(HolziError::InvalidInput {
            reason: field.into(),
        });
    }
    Ok(value)
}

/// Bytes stored as text by the import or by holzi: standard or URL-safe Base64, padded or not.
fn stored_bytes(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    STANDARD
        .decode(text)
        .or_else(|_| STANDARD_NO_PAD.decode(text))
        .or_else(|_| URL_SAFE.decode(text))
        .or_else(|_| URL_SAFE_NO_PAD.decode(text))
        .ok()
}

/// A stored value in the Base64URL form of WebAuthn (as stored when it does not decode).
fn as_b64url(text: &str) -> String {
    stored_bytes(text).map_or_else(|| text.to_owned(), |bytes| URL_SAFE_NO_PAD.encode(bytes))
}

/// The ASCII form of a relying party id as a filter (case and IDNA do not matter).
fn rp_filter(rp_id: &str) -> String {
    match Host::parse(rp_id) {
        Ok(Host::Domain(domain)) => domain.to_ascii_lowercase(),
        _ => rp_id.to_ascii_lowercase(),
    }
}

/// The built-in agent and a caller without a read grant get `Forbidden` (Z2, Z3).
fn require_reach(reader: Reader<'_>) -> Result<()> {
    match authorize_list(reader.caller, reader.grants)? {
        ListView::Agent => Err(HolziError::PasswordsForbidden),
        ListView::Items(_) => Ok(()),
    }
}

/// Whether the caller may read the entry as a passkey's way in: it exists, is not in the trash
/// (for nobody, also not for the user) and lies in the read scope.
fn readable(
    q: &mut impl Query,
    reader: Reader<'_>,
    item_id: &str,
    cache: &mut HashMap<String, bool>,
) -> Result<bool> {
    if let Some(known) = cache.get(item_id) {
        return Ok(*known);
    }
    let ok = match item_state(q, item_id)? {
        Some(state) => {
            !state.in_trash && authorize_read(reader.caller, reader.grants, &state.view()).is_ok()
        }
        None => false,
    };
    cache.insert(item_id.to_owned(), ok);
    Ok(ok)
}

/// One stored passkey with what the service needs, without its key.
struct Row {
    id: String,
    owner: Option<String>,
    credential_id: String,
    rp_id: String,
    rp_name: Option<String>,
    user_name: Option<String>,
    user_display_name: Option<String>,
    user_handle: String,
    nickname: Option<String>,
    algorithm: i64,
    is_discoverable: bool,
    created_at: Option<String>,
    last_used_at: Option<String>,
}

fn rows(q: &mut impl Query, rp_id: Option<&str>) -> Result<Vec<Row>> {
    let filter = rp_id.map(rp_filter);
    Ok(q.query_map(
        "SELECT id, item_id, credential_id, relying_party_id, relying_party_name, user_name, \
                user_display_name, user_handle, nickname, algorithm, is_discoverable, created_at, \
                last_used_at \
         FROM haex_passwords_passkeys \
         WHERE ?1 IS NULL OR lower(relying_party_id) = ?1 ORDER BY rowid",
        params![filter],
        |r| {
            Ok(Row {
                id: r.get(0)?,
                owner: r.get(1)?,
                credential_id: r.get(2)?,
                rp_id: r.get(3)?,
                rp_name: r.get(4)?,
                user_name: r.get(5)?,
                user_display_name: r.get(6)?,
                user_handle: r.get(7)?,
                nickname: r.get(8)?,
                algorithm: r.get(9)?,
                is_discoverable: r.get::<_, i64>(10)? != 0,
                created_at: r.get(11)?,
                last_used_at: r.get(12)?,
            })
        },
    )?)
}

/// A passkey the caller reaches, with the entry it reaches it through.
struct Candidate {
    row: Row,
    item_id: String,
    linked_from: Option<String>,
}

impl Candidate {
    fn header(&self) -> PasskeyHeader {
        PasskeyHeader {
            id: self.row.id.clone(),
            credential_id: as_b64url(&self.row.credential_id),
            rp_id: self.row.rp_id.clone(),
            rp_name: self.row.rp_name.clone(),
            user_name: self.row.user_name.clone(),
            user_display_name: self.row.user_display_name.clone(),
            nickname: self.row.nickname.clone(),
            algorithm: self.row.algorithm,
            is_discoverable: self.row.is_discoverable,
            created_at: self.row.created_at.clone(),
            last_used_at: self.row.last_used_at.clone(),
            item_id: self.item_id.clone(),
            linked_from_item_id: self.linked_from.clone(),
        }
    }
}

/// What narrows the candidates.
#[derive(Default)]
struct Filter<'a> {
    rp_id: Option<&'a str>,
    item_id: Option<&'a str>,
    /// The credential ids (bytes) asked for; empty for any.
    credentials: Vec<Vec<u8>>,
    discoverable_only: bool,
}

/// The passkeys the caller reaches under `filter`, each once: through its own entry when that is
/// readable (and matches the entry filter), else through the first readable link target.
fn candidates(
    q: &mut impl Query,
    reader: Reader<'_>,
    filter: &Filter<'_>,
) -> Result<Vec<Candidate>> {
    let mut cache = HashMap::new();
    let mut out = Vec::new();
    for row in rows(q, filter.rp_id)? {
        if filter.discoverable_only && !row.is_discoverable {
            continue;
        }
        if !filter.credentials.is_empty() {
            let Some(bytes) = stored_bytes(&row.credential_id) else {
                continue;
            };
            if !filter.credentials.contains(&bytes) {
                continue;
            }
        }
        // A passkey without an entry belongs to nobody's scope here (data-model.md).
        let Some(owner) = row.owner.clone() else {
            continue;
        };
        if !readable(q, reader, &owner, &mut cache)? {
            continue;
        }
        if filter.item_id.is_none_or(|item| item == owner) {
            out.push(Candidate {
                row,
                item_id: owner,
                linked_from: None,
            });
            continue;
        }
        let targets: Vec<String> = q.query_map(
            "SELECT item_id FROM haex_passwords_passkey_links WHERE passkey_id = ?1 ORDER BY rowid",
            params![row.id],
            |r| r.get(0),
        )?;
        for target in targets {
            if filter.item_id.is_some_and(|item| item != target) {
                continue;
            }
            if readable(q, reader, &target, &mut cache)? {
                out.push(Candidate {
                    row,
                    item_id: target,
                    linked_from: Some(owner),
                });
                break;
            }
        }
    }
    Ok(out)
}

fn credential_list(field: &'static str, ids: &[String]) -> Result<Vec<Vec<u8>>> {
    ids.iter()
        .map(|id| webauthn::decode_b64url(field, id).map_err(Into::into))
        .collect()
}

/// The caller may add a passkey to this entry.
fn authorize_existing(
    tx: &mut CrdtTransaction<'_>,
    reader: Reader<'_>,
    item_id: &str,
) -> Result<()> {
    required("itemId", item_id)?;
    match item_state(tx, item_id)? {
        Some(state) => {
            authorize_update(reader.caller, reader.grants, &state.view(), None)?;
            Ok(())
        }
        None => {
            // Forbidden without a write grant, otherwise indistinguishable from outside the scope.
            authorize_update(
                reader.caller,
                reader.grants,
                &ItemState {
                    tags: &[],
                    in_trash: false,
                    owner: None,
                },
                None,
            )?;
            Err(HolziError::PasswordsNotFound)
        }
    }
}

/// `passkey_create` (`contracts/passkey-service.md`): a new passkey at an entry the caller may
/// write, or with a new entry (spec 049), for an origin of the relying party.
pub fn create(
    tx: &mut CrdtTransaction<'_>,
    reader: Reader<'_>,
    request: &PasskeyCreateRequest,
) -> Result<PasskeyCreated> {
    match &request.target {
        PasskeyTarget::Existing(item_id) => authorize_existing(tx, reader, item_id)?,
        PasskeyTarget::New(entry) => passkeys_new_entry::authorize(reader, entry)?,
    }
    required("rpId", &request.rp_id)?;
    required("rpName", &request.rp_name)?;
    required("userName", &request.user_name)?;
    required("origin", &request.origin)?;
    let user_handle = webauthn::decode_user_handle(&request.user_handle)?;
    let challenge =
        webauthn::decode_b64url("challenge", required("challenge", &request.challenge)?)?;
    let excluded = credential_list("excludeCredentials", &request.exclude_credentials)?;
    let Some((origin, rp_id)) = webauthn::checked_origin(&request.origin, &request.rp_id) else {
        return Err(HolziError::PasswordsPasskeyOriginMismatch);
    };
    if !excluded.is_empty() {
        let filter = Filter {
            rp_id: Some(&rp_id),
            credentials: excluded,
            ..Filter::default()
        };
        if !candidates(tx, reader, &filter)?.is_empty() {
            return Err(HolziError::PasswordsPasskeyExcluded);
        }
    }
    let algorithm = webauthn::choose_algorithm(&request.pub_key_cred_params)
        .ok_or(HolziError::PasswordsPasskeyUnsupportedAlgorithm)?;

    // Every check passed: only now the new entry, in this transaction (FR-034).
    let item_id = match &request.target {
        PasskeyTarget::Existing(item_id) => item_id.clone(),
        PasskeyTarget::New(entry) => passkeys_new_entry::create(tx, reader, entry)?,
    };
    let key = webauthn::generate_key(algorithm)?;
    let credential_id = webauthn::random_credential_id()?;
    let cose = webauthn::cose_public_key(algorithm, &key.spki)?;
    let auth_data = webauthn::create_auth_data(&rp_id, &credential_id, &cose, request.presence);
    let client_data = webauthn::client_data_json(Ceremony::Create, &challenge, &origin);
    let input = PasskeyInput {
        item_id: Some(item_id.clone()),
        credential_id: STANDARD.encode(&credential_id),
        relying_party_id: rp_id,
        relying_party_name: Some(request.rp_name.clone()),
        user_name: Some(request.user_name.clone()),
        user_display_name: request.user_display_name.clone(),
        user_handle: URL_SAFE_NO_PAD.encode(&user_handle),
        private_key: Zeroizing::new(STANDARD.encode(key.pkcs8.as_slice())),
        public_key: STANDARD.encode(&key.spki),
        algorithm,
        // ponytail: always 0 (research R5); a synced passkey has no global counter.
        sign_count: 0,
        is_discoverable: request.discoverable,
        icon: None,
        color: None,
        nickname: None,
        created_at: None,
        last_used_at: None,
    };
    // 32 random bytes do not repeat; a duplicate would be a broken random source.
    if passkeys::insert(tx, &input)? == InsertOutcome::Duplicate {
        return Err(HolziError::PasswordsConflict {
            reason: "credentialId".into(),
        });
    }
    Ok(PasskeyCreated {
        credential_id: URL_SAFE_NO_PAD.encode(&credential_id),
        attestation_object: URL_SAFE_NO_PAD.encode(webauthn::attestation_object(&auth_data)),
        client_data_json: URL_SAFE_NO_PAD.encode(client_data.as_bytes()),
        public_key_spki: URL_SAFE_NO_PAD.encode(&key.spki),
        algorithm,
        item_id,
        passkey_id: super::ids::passkey_id(&input.credential_id).to_string(),
    })
}

/// `passkey_confirm` (`contracts/passkey-service.md`): signs a challenge with the one passkey that
/// fits, and records `last_used_at` in the same transaction. The counter stays 0 (research R5).
pub fn confirm(
    tx: &mut CrdtTransaction<'_>,
    reader: Reader<'_>,
    request: &PasskeyConfirmRequest,
) -> Result<PasskeyAssertion> {
    require_reach(reader)?;
    required("rpId", &request.rp_id)?;
    required("origin", &request.origin)?;
    let challenge =
        webauthn::decode_b64url("challenge", required("challenge", &request.challenge)?)?;
    let allowed = credential_list("allowCredentials", &request.allow_credentials)?;
    // The origin is checked before anything is read, signed or written.
    let Some((origin, rp_id)) = webauthn::checked_origin(&request.origin, &request.rp_id) else {
        return Err(HolziError::PasswordsPasskeyOriginMismatch);
    };
    let filter = Filter {
        rp_id: Some(&rp_id),
        item_id: request.item_id.as_deref(),
        discoverable_only: allowed.is_empty(),
        credentials: allowed,
    };
    let mut found = candidates(tx, reader, &filter)?;
    if found.len() > 1 {
        return Err(HolziError::PasswordsPasskeyChoiceRequired {
            candidates: found.iter().map(Candidate::header).collect(),
        });
    }
    let candidate = found.pop().ok_or(HolziError::PasswordsNotFound)?;
    let algorithm = candidate.row.algorithm;
    if webauthn::choose_algorithm(&[algorithm]).is_none() {
        return Err(HolziError::PasswordsPasskeyUnsupportedAlgorithm);
    }
    let stored = tx
        .query_row(
            "SELECT private_key FROM haex_passwords_passkeys WHERE id = ?1",
            params![candidate.row.id],
            |r| r.get::<_, String>(0).map(Zeroizing::new),
        )?
        .ok_or(HolziError::PasswordsNotFound)?;
    let pkcs8 = Zeroizing::new(
        stored_bytes(&stored).ok_or(HolziError::PasswordsPasskeyUnsupportedAlgorithm)?,
    );
    let auth_data = webauthn::get_auth_data(&rp_id, request.presence);
    let client_data = webauthn::client_data_json(Ceremony::Get, &challenge, &origin);
    let signature = webauthn::sign(algorithm, &pkcs8, &auth_data, client_data.as_bytes())?;
    // Same transaction: if this write fails, no signature leaves.
    tx.execute(
        "UPDATE haex_passwords_passkeys SET last_used_at = ?1 WHERE id = ?2",
        params![super::clock::now(), candidate.row.id],
    )?;
    let user_handle = Some(as_b64url(&candidate.row.user_handle)).filter(|h| !h.is_empty());
    Ok(PasskeyAssertion {
        credential_id: as_b64url(&candidate.row.credential_id),
        authenticator_data: URL_SAFE_NO_PAD.encode(&auth_data),
        client_data_json: URL_SAFE_NO_PAD.encode(client_data.as_bytes()),
        signature: URL_SAFE_NO_PAD.encode(signature),
        user_handle,
        item_id: candidate.item_id,
    })
}

/// `passkey_list` (`contracts/passkey-service.md`): the headers of the passkeys the caller reaches.
pub fn list(
    q: &mut impl Query,
    reader: Reader<'_>,
    request: &PasskeyListRequest,
) -> Result<Vec<PasskeyHeader>> {
    require_reach(reader)?;
    let filter = Filter {
        rp_id: request.rp_id.as_deref().filter(|rp| !rp.trim().is_empty()),
        item_id: request.item_id.as_deref(),
        discoverable_only: request.discoverable_only,
        credentials: Vec::new(),
    };
    Ok(candidates(q, reader, &filter)?
        .iter()
        .map(Candidate::header)
        .collect())
}

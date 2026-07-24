use super::*;

use crate::database::error::DatabaseError;
use crate::AppState;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::SigningKey;
use serde_json::Value as JsonValue;
use tauri::State;

// ---------------------------------------------------------------------------
// Default identity bootstrap
//
// Every vault needs at least one "own" identity (a row in haex_identities with
// a non-null private_key) before anything identity-backed can happen:
// haex_spaces.owner_identity_id is NOT NULL, UCAN signing needs a private key,
// and the JS side looks one up on vault mount.
//
// The UI flow (vault.vue onMounted → ensureDefaultIdentityAsync) handles this
// for humans, but direct-Tauri paths (e.g. E2E tests that invoke
// create_encrypted_database / open_encrypted_database without navigating)
// would otherwise leave the vault without an identity. Seeding it here
// guarantees that every freshly-opened vault is immediately usable.
//
// The key format mirrors the JS side so JS can transparently load and sign
// with keys created here:
//   - private_key: Base64-encoded 48-byte Ed25519 PKCS8 (16-byte prefix + 32-byte seed)
//   - did: `did:key:z` + base58btc(0xed01 || raw-public-key)
// ---------------------------------------------------------------------------

// Ed25519 multicodec tag used in did:key format.
const DEFAULT_IDENTITY_ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

// PKCS8 ASN.1 wrapper used by WebCrypto's exportKey('pkcs8') for Ed25519.
// SEQUENCE(46) → INTEGER(0) → AlgorithmId(OID 1.3.101.112) → OCTET STRING(34 → OCTET STRING(32))
const DEFAULT_IDENTITY_ED25519_PKCS8_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// Default English name for auto-seeded identities. JS's
/// `ensureDefaultIdentityAsync` recognises this value and will re-localise /
/// generate an avatar on first UI mount, so we don't need locale awareness
/// in Rust.
const DEFAULT_IDENTITY_NAME: &str = "My Identity";

fn generate_default_identity_material() -> (String, String) {
    let mut seed = [0u8; 32];
    rand::fill(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let public_key = signing_key.verifying_key();

    // did:key:z<base58btc(multicodec || pubkey)>
    let mut did_bytes = Vec::with_capacity(34);
    did_bytes.extend_from_slice(&DEFAULT_IDENTITY_ED25519_MULTICODEC);
    did_bytes.extend_from_slice(public_key.as_bytes());
    let did = format!("did:key:z{}", bs58::encode(did_bytes).into_string());

    // PKCS8(prefix || seed), Base64 (matches crypto.subtle.exportKey('pkcs8')).
    let mut pkcs8 = Vec::with_capacity(48);
    pkcs8.extend_from_slice(&DEFAULT_IDENTITY_ED25519_PKCS8_PREFIX);
    pkcs8.extend_from_slice(&seed);
    let private_key_b64 = BASE64.encode(&pkcs8);

    (did, private_key_b64)
}

/// Ensures the currently open vault has at least one own identity. Idempotent:
/// becomes a no-op when a row with private_key IS NOT NULL already exists.
pub(super) fn ensure_default_identity(state: &State<'_, AppState>) -> Result<(), DatabaseError> {
    // CRDT-aware existence check: select_with_crdt strips tombstoned rows,
    // so a previously-deleted default identity doesn't suppress re-seeding.
    let existing = core::select_with_crdt(
        "SELECT id FROM haex_identities WHERE private_key IS NOT NULL LIMIT 1".to_string(),
        vec![],
        &state.db,
    )?;
    if !existing.is_empty() {
        return Ok(());
    }

    let (did, private_key_b64) = generate_default_identity_material();
    let id = uuid::Uuid::new_v4().to_string();

    let hlc_service = state.lock_or_fail(
        &state.hlc,
        crate::critical::CriticalFailureCode::HlcMutexPoisoned,
        "database::ensure_default_identity",
        serde_json::json!({}),
    )?;

    // source='own' so downstream consumers (e.g. the Phase 2
    // haex_devices.owner_did join) can distinguish own identities from
    // contact / space-member rows. Pre-Phase-2 the seed was tagged 'contact'
    // because code only looked at private_key — the Phase 2 matrix view
    // also checks source.
    core::execute_with_crdt(
        "INSERT INTO haex_identities (id, did, name, source, private_key) VALUES (?1, ?2, ?3, 'own', ?4)".to_string(),
        vec![
            JsonValue::String(id),
            JsonValue::String(did.clone()),
            JsonValue::String(DEFAULT_IDENTITY_NAME.to_string()),
            JsonValue::String(private_key_b64),
        ],
        &state.db,
        &hlc_service,
        &state.column_sig_key_cache,
    )?;

    println!(
        "[IDENTITY] ✅ default identity seeded ({})",
        &did[..30.min(did.len())]
    );
    Ok(())
}

//! Unit tests for the three haex-crdt provider adapters.
//!
//! Illustrative signatures in the integration plan were adapted to match
//! the real haex-crdt trait shapes:
//!
//! - `SignatureProvider::sign_column(preimage: &[u8]) -> Result<Vec<u8>>`
//! - `SignatureProvider::verify_column(preimage: &[u8], sig: &JsonValue) -> Result<()>`
//! - `MigrationSource::list_migrations() -> Result<Vec<MigrationName>>`
//!
//! The plan's `app_migrations()` / multi-arg `sign_column` variants do not
//! exist on the trait; the semantic tests below verify the same properties
//! against the real trait surface.

use super::*;

use haex_crdt::{DeviceIdProvider, MigrationSource, SignatureProvider};
use serde_json::json;

#[test]
fn device_id_provider_returns_stable_uuid() {
    let provider = HaexVaultDeviceIdProvider::from_state_test_seed(&[0xAB; 32]);
    let a = provider.device_id().unwrap();
    let b = provider.device_id().unwrap();
    assert_eq!(a, b, "device id must be stable across calls");
}

#[test]
fn device_id_provider_seed_bytes_flow_into_uuid() {
    // Sanity: the seed's first 16 bytes are the UUID bytes. This locks in
    // the test-seed contract so a future refactor of the derivation doesn't
    // silently change what tests assert against.
    let mut seed = [0u8; 32];
    for (i, b) in seed.iter_mut().enumerate() {
        *b = i as u8;
    }
    let provider = HaexVaultDeviceIdProvider::from_state_test_seed(&seed);
    let uuid = provider.device_id().unwrap();
    let mut expected = [0u8; 16];
    expected.copy_from_slice(&seed[..16]);
    assert_eq!(uuid.as_bytes(), &expected);
}

#[test]
fn signature_provider_signs_and_verifies_column_roundtrip() {
    let provider = HaexVaultSignatureProvider::for_test();
    let preimage = b"batch-1-round-trip-preimage";

    let sig_bytes = provider.sign_column(preimage).unwrap();
    assert!(
        !sig_bytes.is_empty(),
        "real provider must not produce empty signatures"
    );

    let sig_json = json!({ "bytes": hex::encode(&sig_bytes) });
    provider
        .verify_column(preimage, &sig_json)
        .expect("signature roundtrip");
}

#[test]
fn signature_provider_rejects_tampered_preimage() {
    // The round-trip test above passes even if verify_column silently
    // accepted everything. This test locks in real verification: mutating
    // the preimage must fail against a signature produced from the
    // original.
    let provider = HaexVaultSignatureProvider::for_test();
    let preimage = b"original-preimage";
    let sig_bytes = provider.sign_column(preimage).unwrap();
    let sig_json = json!({ "bytes": hex::encode(&sig_bytes) });

    let tampered = b"tampered-preimage";
    let err = provider
        .verify_column(tampered, &sig_json)
        .expect_err("verify must reject tampered preimage");
    // The provider surfaces failures via `Error::Message` for Batch 1;
    // the exact string is not part of the contract, only that it errors.
    let rendered = err.to_string();
    assert!(
        rendered.contains("signature verify failed"),
        "unexpected error surface: {rendered}"
    );
}

#[test]
fn signature_provider_author_id_is_non_empty_for_real_provider() {
    // NoopSignatureProvider returns AuthorId::anonymous() (empty string).
    // A real provider — even the test one — must expose a stable identity
    // so consumers can distinguish signed-by-self from anonymous writes.
    let provider = HaexVaultSignatureProvider::for_test();
    let author = provider.author_id();
    assert!(!author.0.is_empty(), "test provider must expose an author");
}

#[test]
fn migration_source_lists_shipped_migrations_in_lexicographic_order() {
    let source = HaexVaultMigrationSource::from_embedded().unwrap();
    let listed = source.list_migrations().unwrap();

    assert!(
        !listed.is_empty(),
        "list_migrations must expose the shipped drizzle + manual migrations"
    );

    // The trait contract requires stable total order across calls.
    let listed_again = source.list_migrations().unwrap();
    assert_eq!(
        listed, listed_again,
        "list_migrations must be deterministic"
    );

    // Lexicographic order — a BTreeMap gives this for free but the contract
    // is worth locking in against a future refactor.
    let mut sorted = listed.clone();
    sorted.sort();
    assert_eq!(listed, sorted, "list_migrations must be lexicographic");
}

#[test]
fn migration_source_load_migration_returns_sql_body() {
    let source = HaexVaultMigrationSource::from_embedded().unwrap();
    let listed = source.list_migrations().unwrap();
    let first = listed
        .first()
        .expect("shipped migrations must not be empty");

    let sql = source.load_migration(first).unwrap();
    assert!(
        !sql.trim().is_empty(),
        "migration `{}` had empty SQL",
        first.as_str()
    );
}

#[test]
fn migration_source_from_migrations_dir_lists_shipped_migrations() {
    // Proves the production constructor is usable, not just the
    // `#[cfg(test)]` convenience. Batch 5 will pass a real
    // `AppHandle`-resolved path here; the parser under the hood is the
    // same one `from_embedded` already covers.
    use std::path::PathBuf;

    let manifest_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = HaexVaultMigrationSource::from_migrations_dir(manifest_root).unwrap();
    let listed = source.list_migrations().unwrap();
    assert!(
        !listed.is_empty(),
        "production constructor must expose the shipped migrations"
    );
}

#[test]
fn migration_source_load_missing_reports_consumer_owned_journal() {
    let source = HaexVaultMigrationSource::from_embedded().unwrap();
    let missing = "9999_definitely_not_shipped".into();
    let err = source
        .load_migration(&missing)
        .expect_err("missing migration must error");
    match err {
        haex_crdt::Error::MigrationMissingFromSource { journal, name } => {
            assert_eq!(journal, haex_crdt::MigrationJournal::ConsumerOwned);
            assert_eq!(name, "9999_definitely_not_shipped");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

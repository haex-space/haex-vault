//! Regression coverage for the provider model cache
//! (`storage::models::replace_provider_models`).
//!
//! `models` is CRDT-tracked, so a refresh must not delete and re-insert
//! the rows it is about to keep. The BEFORE-DELETE trigger appends a
//! tombstone to `haex_deleted_rows` carrying `current_hlc()`, that value
//! is pinned for the whole transaction, and the re-INSERT therefore
//! lands on the very same HLC — a peer resolves that tie in favour of
//! the delete, so every refreshed model would vanish on the other
//! device. These tests pin the behaviour against the real vault so the
//! delete-then-insert shape cannot come back unnoticed.

// These tests read raw vault state (counts, CRDT columns, the delete log) that
// the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::{
    Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey, DEFAULT_TRIGGER_VERSION,
};
use uuid::Uuid;

use haex_lib::identity::{haex_migration_source, installation_id_path, HaexBootstrap};
use haex_lib::storage::models::{self as models_store, IntegrityStatus, ModelRow, SourceKind};
use haex_lib::storage::query;

const PASSPHRASE: &str = "provider-models-integration-test";

/// Opens a fresh vault under `dir` with the production migration set.
fn open_vault(dir: &Path) -> Database {
    let db_path: PathBuf = dir.join("vault.db");
    let installation_id = installation_id_path(dir);
    Database::open(DatabaseConfig {
        path: db_path,
        key: SqlCipherKey::new(PASSPHRASE),
        create_if_missing: true,
        bootstrap: Arc::new(HaexBootstrap::new(installation_id).with_alias("test")),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: haex_migration_source(),
        trigger_version: DEFAULT_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("vault open")
}

/// One cache row as `refresh_provider_models` composes it.
fn model(provider_id: Uuid, remote_id: &str) -> ModelRow {
    ModelRow {
        id: format!("{provider_id}:{remote_id}"),
        provider_id,
        name: remote_id.to_string(),
        context_window: Some(200_000),
        fetched_at: Some(1),
        tokenizer_repo: None,
        hf_repo: None,
        hf_filename: None,
        hf_revision: None,
        hf_revision_ref: None,
        file_sha256: None,
        integrity_status: IntegrityStatus::Unknown,
        source_kind: SourceKind::Provider,
        capabilities: None,
    }
}

/// How many delete-log entries the `models` table has accumulated.
fn tombstones(db: &Database) -> i64 {
    db.with_connection(|conn| {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'models'",
            [],
            |r| r.get(0),
        )?;
        Ok(n)
    })
    .expect("count tombstones")
}

/// The cached model ids for one provider, ordered by name.
fn cached_ids(db: &Database, provider_id: Uuid) -> Vec<String> {
    query::read(db, |r| {
        Ok(models_store::list_models_by_provider(r, provider_id)
            .expect("list models")
            .into_iter()
            .map(|m| m.id)
            .collect())
    })
    .expect("cached ids")
}

#[test]
/// A refresh that reports the same models again must leave the CRDT
/// delete-log untouched — no tombstone may shadow the refreshed rows.
fn refresh_keeps_surviving_rows_out_of_the_delete_log() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider_id = Uuid::new_v4();

    let fresh = vec![model(provider_id, "opus"), model(provider_id, "haiku")];
    db.write(|tx| {
        models_store::replace_provider_models(tx, provider_id, &fresh).expect("first refresh");
        Ok(())
    })
    .expect("first refresh");
    assert_eq!(tombstones(&db), 0, "first refresh deletes nothing");

    // Second refresh, identical listing — the classic delete-all +
    // re-insert shape would emit one tombstone per surviving model.
    db.write(|tx| {
        models_store::replace_provider_models(tx, provider_id, &fresh).expect("second refresh");
        Ok(())
    })
    .expect("second refresh");

    assert_eq!(
        tombstones(&db),
        0,
        "a model that survives the refresh must not be tombstoned — the tombstone \
         carries the transaction HLC and would shadow the re-insert on every peer"
    );
    let mut ids = cached_ids(&db, provider_id);
    ids.sort();
    assert_eq!(
        ids,
        vec![
            format!("{provider_id}:haiku"),
            format!("{provider_id}:opus"),
        ]
    );
}

#[test]
/// A model the provider stopped reporting is dropped from the cache.
fn refresh_removes_models_the_provider_no_longer_reports() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider_id = Uuid::new_v4();

    db.write(|tx| {
        let initial = vec![model(provider_id, "opus"), model(provider_id, "retired")];
        models_store::replace_provider_models(tx, provider_id, &initial).expect("seed");
        Ok(())
    })
    .expect("seed");

    db.write(|tx| {
        let shrunk = vec![model(provider_id, "opus")];
        models_store::replace_provider_models(tx, provider_id, &shrunk).expect("shrink");
        Ok(())
    })
    .expect("shrink");

    assert_eq!(
        cached_ids(&db, provider_id),
        vec![format!("{provider_id}:opus")]
    );
    assert_eq!(
        tombstones(&db),
        1,
        "exactly the retired model is tombstoned"
    );
}

#[test]
/// Cursor pagination can hand the same model back twice when the remote
/// catalog changes mid-listing. That must not abort the refresh on a
/// primary-key conflict.
fn refresh_tolerates_a_duplicate_model_id() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider_id = Uuid::new_v4();

    db.write(|tx| {
        let overlapping = vec![
            model(provider_id, "opus"),
            model(provider_id, "haiku"),
            model(provider_id, "opus"),
        ];
        models_store::replace_provider_models(tx, provider_id, &overlapping)
            .expect("duplicate ids must not fail the refresh");
        Ok(())
    })
    .expect("refresh with duplicates");

    let mut ids = cached_ids(&db, provider_id);
    ids.sort();
    assert_eq!(
        ids,
        vec![
            format!("{provider_id}:haiku"),
            format!("{provider_id}:opus"),
        ]
    );
}

#[test]
/// Rows belonging to a different provider are never touched by a
/// refresh — the local provider's downloaded models must survive an
/// api_key provider's refresh.
fn refresh_leaves_other_providers_alone() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let refreshed = Uuid::new_v4();
    let other = Uuid::new_v4();

    db.write(|tx| {
        tx.execute(
            "INSERT INTO models (id, provider_id, name, context_window, fetched_at, \
             tokenizer_repo) \
             VALUES (?1, ?2, 'local gguf', 4096, NULL, 'org/tok')",
            params!["qwen2.5-0.5b", other.to_string()],
        )?;
        models_store::replace_provider_models(tx, refreshed, &[model(refreshed, "opus")])
            .expect("refresh");
        Ok(())
    })
    .expect("refresh");

    assert_eq!(cached_ids(&db, other), vec!["qwen2.5-0.5b".to_string()]);
    assert_eq!(tombstones(&db), 0);
}

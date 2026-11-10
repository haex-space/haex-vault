//! Storage-level regression coverage for spec 005 (free HuggingFace model
//! discovery): catalog, imported and free-HF rows must coexist in the
//! shared `models` table, and the migration-default `source_kind` must be
//! reclassified for pre-existing local rows without ever touching a true
//! `api_key` provider row.
//!
//! `list_installed_models` / `load_model` / `delete_installed_model` also
//! resolve on-disk paths through a live `AppHandle`, which — like the rest
//! of this integration suite (see `chat_model_preload.rs`) — this project
//! does not mock; those full desktop scenarios are covered manually via
//! `specs/005-huggingface-model-discovery/quickstart.md` §5/§6. What is
//! tested here is the part that does not need an `AppHandle`: the shared
//! storage boundary every one of those commands reads and writes through.

use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::storage::models::{self as models_store, IntegrityStatus, ModelRow, SourceKind};
use holzi_lib::storage::providers::{insert_provider, Provider, ProviderCapability, ProviderKind};
use holzi_lib::storage::query;
use uuid::Uuid;

async fn open_test_db(name: &str) -> (tempfile::TempDir, Arc<Database>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();
    let name = name.to_string();
    let db = tokio::task::spawn_blocking(move || {
        Database::open(DatabaseConfig {
            path: path.join("vault.db"),
            key: SqlCipherKey::new(&name),
            create_if_missing: true,
            bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(&path))),
            signature_provider: Arc::new(NoopSignatureProvider),
            migration_source: holzi_migration_source(),
            trigger_version: HOLZI_TRIGGER_VERSION,
            max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
            max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
        })
        .expect("open vault")
    })
    .await
    .expect("open join");
    (dir, Arc::new(db))
}

fn hf_row(id: &str, provider_id: Uuid) -> ModelRow {
    ModelRow {
        id: id.to_string(),
        provider_id,
        name: "Free HF model".into(),
        context_window: Some(8192),
        fetched_at: Some(1),
        tokenizer_repo: Some("owner/tokenizer".into()),
        hf_repo: Some("owner/repo".into()),
        hf_filename: Some("model.Q4_K_M.gguf".into()),
        hf_revision: Some("a".repeat(40)),
        hf_revision_ref: Some("main".into()),
        file_sha256: Some("b".repeat(64)),
        integrity_status: IntegrityStatus::Verified,
        source_kind: SourceKind::Huggingface,
        capabilities: None,
    }
}

fn catalog_row(id: &str, provider_id: Uuid) -> ModelRow {
    ModelRow {
        id: id.to_string(),
        provider_id,
        name: "Qwen3 0.6B".into(),
        context_window: Some(4096),
        fetched_at: None,
        tokenizer_repo: Some("qwen/tokenizer".into()),
        hf_repo: None,
        hf_filename: None,
        hf_revision: None,
        hf_revision_ref: None,
        file_sha256: Some("c".repeat(64)),
        integrity_status: IntegrityStatus::Verified,
        source_kind: SourceKind::Catalog,
        capabilities: None,
    }
}

#[tokio::test]
async fn catalog_import_and_huggingface_rows_coexist_and_roundtrip() {
    let (_dir, db) = open_test_db("hf-and-catalog-coexist").await;
    let provider_id = Uuid::new_v4();

    tokio::task::spawn_blocking(move || {
        db.write(|tx| {
            insert_provider(
                tx,
                &Provider {
                    id: provider_id,
                    kind: ProviderKind::Local,
                    adapter: None,
                    name: "Local (llama.cpp)".into(),
                    base_url: None,
                    credentials: None,
                    created_at: 0,
                    capability: ProviderCapability::Chat,
                },
            )?;
            models_store::upsert_model(tx, &catalog_row("qwen3-0.6b", provider_id))?;
            models_store::upsert_model(tx, &hf_row("hf-freerepo", provider_id))?;
            let imported = ModelRow {
                id: "imported-local".into(),
                provider_id,
                name: "Imported".into(),
                context_window: None,
                fetched_at: None,
                tokenizer_repo: Some("owner/tok".into()),
                hf_repo: None,
                hf_filename: None,
                hf_revision: None,
                hf_revision_ref: None,
                file_sha256: Some("d".repeat(64)),
                integrity_status: IntegrityStatus::Verified,
                source_kind: SourceKind::Imported,
                capabilities: None,
            };
            models_store::upsert_model(tx, &imported)?;
            Ok::<_, haex_crdt::Error>(())
        })
        .expect("seed rows");

        let all = query::read(&db, |r| models_store::list_all_models(r)).expect("list all models");
        assert_eq!(all.len(), 3);

        let catalog = all
            .iter()
            .find(|r| r.id == "qwen3-0.6b")
            .expect("catalog row");
        assert_eq!(catalog.source_kind, SourceKind::Catalog);
        assert!(catalog.hf_repo.is_none());

        let hf = all.iter().find(|r| r.id == "hf-freerepo").expect("hf row");
        assert_eq!(hf.source_kind, SourceKind::Huggingface);
        assert_eq!(hf.hf_repo.as_deref(), Some("owner/repo"));
        assert_eq!(hf.hf_filename.as_deref(), Some("model.Q4_K_M.gguf"));
        assert_eq!(hf.hf_revision.as_deref(), Some("a".repeat(40).as_str()));
        assert_eq!(hf.hf_revision_ref.as_deref(), Some("main"));
        assert_eq!(hf.file_sha256.as_deref(), Some("b".repeat(64).as_str()));
        assert_eq!(hf.integrity_status, IntegrityStatus::Verified);

        let imported = all
            .iter()
            .find(|r| r.id == "imported-local")
            .expect("imported row");
        assert_eq!(imported.source_kind, SourceKind::Imported);
        assert!(imported.hf_repo.is_none());
    })
    .await
    .expect("assert join");
}

#[tokio::test]
async fn a_hash_mismatch_is_never_silently_upgraded_to_verified() {
    // Regression for research.md Entscheidung 6: only an explicit
    // download/update/import may set `file_sha256`; the integrity-status
    // helper used by the pre-load check must never touch it, whichever
    // status it writes.
    let (_dir, db) = open_test_db("integrity-status-narrow-update").await;
    let provider_id = Uuid::new_v4();

    tokio::task::spawn_blocking(move || {
        db.write(|tx| {
            insert_provider(
                tx,
                &Provider {
                    id: provider_id,
                    kind: ProviderKind::Local,
                    adapter: None,
                    name: "Local (llama.cpp)".into(),
                    base_url: None,
                    credentials: None,
                    created_at: 0,
                    capability: ProviderCapability::Chat,
                },
            )?;
            models_store::upsert_model(tx, &hf_row("hf-freerepo", provider_id))?;
            Ok::<_, haex_crdt::Error>(())
        })
        .expect("seed row");

        let expected_hash = "b".repeat(64);

        db.write(|tx| {
            models_store::set_integrity_status(tx, "hf-freerepo", IntegrityStatus::Untrusted)
        })
        .expect("mark untrusted");
        let row = query::read(&db, |r| models_store::get_model(r, "hf-freerepo"))
            .expect("get after untrusted")
            .expect("row exists");
        assert_eq!(row.integrity_status, IntegrityStatus::Untrusted);
        assert_eq!(row.file_sha256.as_deref(), Some(expected_hash.as_str()));

        db.write(|tx| {
            models_store::set_integrity_status(tx, "hf-freerepo", IntegrityStatus::Verified)
        })
        .expect("mark verified again");
        let row = query::read(&db, |r| models_store::get_model(r, "hf-freerepo"))
            .expect("get after verified")
            .expect("row exists");
        assert_eq!(row.integrity_status, IntegrityStatus::Verified);
        assert_eq!(row.file_sha256.as_deref(), Some(expected_hash.as_str()));
    })
    .await
    .expect("assert join");
}

#[tokio::test]
async fn backfill_source_kind_reclassifies_local_rows_but_never_touches_a_provider_row() {
    let (_dir, db) = open_test_db("backfill-source-kind").await;
    let local_provider_id = Uuid::new_v4();
    let api_provider_id = Uuid::new_v4();

    tokio::task::spawn_blocking(move || {
        db.write(|tx| {
            insert_provider(
                tx,
                &Provider {
                    id: local_provider_id,
                    kind: ProviderKind::Local,
                    adapter: None,
                    name: "Local (llama.cpp)".into(),
                    base_url: None,
                    credentials: None,
                    created_at: 0,
                    capability: ProviderCapability::Chat,
                },
            )?;
            insert_provider(
                tx,
                &Provider {
                    id: api_provider_id,
                    kind: ProviderKind::ApiKey,
                    adapter: Some("anthropic".into()),
                    name: "Anthropic".into(),
                    base_url: None,
                    credentials: None,
                    created_at: 0,
                    capability: ProviderCapability::Chat,
                },
            )?;
            // Pre-0015 rows: migration default `source_kind = provider`
            // regardless of which provider actually owns them.
            let legacy_catalog = ModelRow {
                source_kind: SourceKind::Provider,
                ..catalog_row("qwen3-0.6b", local_provider_id)
            };
            let legacy_import = ModelRow {
                id: "legacy-import".into(),
                source_kind: SourceKind::Provider,
                hf_repo: None,
                hf_filename: None,
                hf_revision: None,
                hf_revision_ref: None,
                ..hf_row("legacy-import", local_provider_id)
            };
            let real_provider_model = ModelRow {
                id: format!("{api_provider_id}:claude-x"),
                provider_id: api_provider_id,
                name: "Claude X".into(),
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
            };
            models_store::upsert_model(tx, &legacy_catalog)?;
            models_store::upsert_model(tx, &legacy_import)?;
            models_store::upsert_model(tx, &real_provider_model)?;
            Ok::<_, haex_crdt::Error>(())
        })
        .expect("seed legacy rows");

        db.write(|tx| models_store::backfill_source_kind(tx, local_provider_id, &["qwen3-0.6b"]))
            .expect("backfill");

        let get = |id: &str| -> ModelRow {
            query::read(&db, |r| models_store::get_model(r, id))
                .expect("get")
                .unwrap_or_else(|| panic!("row {id} must exist"))
        };

        assert_eq!(get("qwen3-0.6b").source_kind, SourceKind::Catalog);
        assert_eq!(get("legacy-import").source_kind, SourceKind::Imported);
        // The real provider row must never be reclassified as catalog or
        // imported just because backfill ran.
        assert_eq!(
            get(&format!("{api_provider_id}:claude-x")).source_kind,
            SourceKind::Provider
        );

        // Idempotent: a second pass changes nothing further.
        let updated = db
            .write(|tx| models_store::backfill_source_kind(tx, local_provider_id, &["qwen3-0.6b"]))
            .expect("second backfill");
        assert_eq!(updated, 0);
    })
    .await
    .expect("assert join");
}

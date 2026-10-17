//! Database-backed coverage for the `models.capabilities_json` column (spec
//! 012-unified-model-capabilities): round trip through the typed reads,
//! whole-record overwrite on refresh, tolerance of unreadable stored values,
//! and the lazy backfill for local rows.
//!
//! These run against a real vault because `upsert_model` and the backfill
//! stamp rows with `current_hlc()`, which only exists on an open
//! `haex_crdt::Database`.

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

use holzi_lib::adapters::AttachmentKind;
use holzi_lib::identity::{holzi_migration_source, installation_id_path, HolziBootstrap};
use holzi_lib::model_capabilities::{
    ModelCapabilities, ReasoningControl, ReasoningOption, ThinkingStyle, ToolSupport, ToolUse,
    ToolUseBasis,
};
use holzi_lib::storage::models::{self as models_store, IntegrityStatus, ModelRow, SourceKind};
use holzi_lib::storage::query;

const PASSPHRASE: &str = "model-capabilities-storage-test";

/// Opens a fresh vault under `dir` with the production migration set.
fn open_vault(dir: &Path) -> Database {
    let db_path: PathBuf = dir.join("vault.db");
    let installation_id = installation_id_path(dir);
    Database::open(DatabaseConfig {
        path: db_path,
        key: SqlCipherKey::new(PASSPHRASE),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id).with_alias("test")),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: DEFAULT_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("vault open")
}

fn row(
    provider_id: Uuid,
    id: &str,
    source_kind: SourceKind,
    capabilities: Option<ModelCapabilities>,
) -> ModelRow {
    ModelRow {
        id: id.to_string(),
        provider_id,
        name: id.to_string(),
        context_window: None,
        fetched_at: Some(1),
        tokenizer_repo: None,
        hf_repo: None,
        hf_filename: None,
        hf_revision: None,
        hf_revision_ref: None,
        file_sha256: None,
        integrity_status: IntegrityStatus::Unknown,
        source_kind,
        capabilities,
    }
}

fn claude_capabilities() -> ModelCapabilities {
    ModelCapabilities {
        reasoning: Some(ReasoningControl::presets(vec![
            ReasoningOption {
                id: "low".to_string(),
                label: "low".to_string(),
            },
            ReasoningOption {
                id: "max".to_string(),
                label: "max".to_string(),
            },
        ])),
        accepted_attachment_kinds: Some(vec![AttachmentKind::Text, AttachmentKind::Image]),
        thinking_style: Some(ThinkingStyle::Adaptive),
        tool_use: Some(ToolUse::new(ToolSupport::Supported, ToolUseBasis::Provider)),
        user: None,
    }
}

fn upsert(db: &Database, m: &ModelRow) {
    db.write(|tx| {
        models_store::upsert_model(tx, m).expect("upsert model");
        Ok(())
    })
    .expect("upsert");
}

fn get(db: &Database, id: &str) -> ModelRow {
    query::read(db, |r| {
        Ok(models_store::get_model(r, id).expect("get model"))
    })
    .expect("get")
    .expect("row exists")
}

/// Raw stored value, bypassing the typed reads.
fn stored_column(db: &Database, id: &str) -> Option<String> {
    db.with_connection(|conn| {
        Ok(conn
            .query_row(
                "SELECT capabilities_json FROM models WHERE id = ?1",
                params![id],
                |r| r.get::<_, Option<String>>(0),
            )
            .expect("read column"))
    })
    .expect("stored column")
}

fn overwrite_column(db: &Database, id: &str, value: Option<&str>) {
    db.write(|tx| {
        tx.execute(
            "UPDATE models SET capabilities_json = ?1 WHERE id = ?2",
            params![value, id],
        )
        .expect("overwrite column");
        Ok(())
    })
    .expect("overwrite");
}

#[test]
fn a_populated_record_round_trips_through_every_typed_read() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider = Uuid::new_v4();
    let id = format!("{provider}:claude-opus-5");
    upsert(
        &db,
        &row(
            provider,
            &id,
            SourceKind::Provider,
            Some(claude_capabilities()),
        ),
    );

    assert_eq!(get(&db, &id).capabilities, Some(claude_capabilities()));
    let by_provider = query::read(&db, |r| {
        Ok(models_store::list_models_by_provider(r, provider).expect("list"))
    })
    .expect("by provider");
    assert_eq!(by_provider[0].capabilities, Some(claude_capabilities()));
    let all = query::read(&db, |r| {
        Ok(models_store::list_all_models(r).expect("list all"))
    })
    .expect("all");
    assert_eq!(
        all.iter()
            .find(|m| m.id == id)
            .expect("listed")
            .capabilities,
        Some(claude_capabilities())
    );
}

#[test]
fn an_undetermined_record_is_stored_as_null_and_reads_back_as_none() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider = Uuid::new_v4();
    let id = format!("{provider}:codex");
    upsert(
        &db,
        &row(
            provider,
            &id,
            SourceKind::Provider,
            Some(ModelCapabilities::default()),
        ),
    );

    assert_eq!(stored_column(&db, &id), None);
    assert_eq!(get(&db, &id).capabilities, None);
}

#[test]
fn a_second_upsert_replaces_the_stored_record_entirely_including_with_none() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider = Uuid::new_v4();
    let id = format!("{provider}:claude-opus-5");
    upsert(
        &db,
        &row(
            provider,
            &id,
            SourceKind::Provider,
            Some(claude_capabilities()),
        ),
    );

    let newer = ModelCapabilities {
        reasoning: Some(ReasoningControl::ModelManaged),
        ..ModelCapabilities::default()
    };
    upsert(
        &db,
        &row(provider, &id, SourceKind::Provider, Some(newer.clone())),
    );
    assert_eq!(get(&db, &id).capabilities, Some(newer));

    upsert(&db, &row(provider, &id, SourceKind::Provider, None));
    assert_eq!(get(&db, &id).capabilities, None);
    assert_eq!(stored_column(&db, &id), None);
}

#[test]
fn an_unreadable_stored_value_reads_as_none_and_keeps_the_row_listed() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider = Uuid::new_v4();
    let id = format!("{provider}:claude-opus-5");
    upsert(
        &db,
        &row(
            provider,
            &id,
            SourceKind::Provider,
            Some(claude_capabilities()),
        ),
    );
    overwrite_column(&db, &id, Some("{ this is not json"));

    let read = get(&db, &id);
    assert_eq!(read.capabilities, None);
    assert_eq!(read.name, id, "every other column stays intact");
    let listed = query::read(&db, |r| {
        Ok(models_store::list_models_by_provider(r, provider).expect("list"))
    })
    .expect("list");
    assert_eq!(
        listed.len(),
        1,
        "the corrupt row must not break the listing"
    );
}

#[test]
fn a_stored_presets_record_with_no_options_reads_back_as_unavailable() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider = Uuid::new_v4();
    let id = format!("{provider}:claude-opus-5");
    upsert(&db, &row(provider, &id, SourceKind::Provider, None));
    overwrite_column(
        &db,
        &id,
        Some(r#"{"reasoning":{"kind":"presets","options":[]}}"#),
    );

    assert_eq!(
        get(&db, &id).capabilities.and_then(|c| c.reasoning),
        Some(ReasoningControl::Unavailable)
    );
}

#[test]
fn backfill_fills_only_null_local_rows_and_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let local = Uuid::new_v4();
    let remote = Uuid::new_v4();
    let remote_id = format!("{remote}:claude-opus-5");
    let already = ModelCapabilities {
        reasoning: Some(ReasoningControl::Unavailable),
        accepted_attachment_kinds: Some(vec![AttachmentKind::Image]),
        thinking_style: None,
        tool_use: None,
        user: None,
    };
    upsert(
        &db,
        &row(local, "qwen3-4b-instruct", SourceKind::Catalog, None),
    );
    upsert(&db, &row(local, "qwen2.5-0.5b", SourceKind::Imported, None));
    upsert(
        &db,
        &row(
            local,
            "already-known",
            SourceKind::Imported,
            Some(already.clone()),
        ),
    );
    upsert(&db, &row(remote, &remote_id, SourceKind::Provider, None));

    let updated = db
        .write(|tx| Ok(models_store::backfill_local_capabilities(tx, local).expect("backfill")))
        .expect("first backfill");

    assert_eq!(updated, 2);
    assert_eq!(
        get(&db, "qwen3-4b-instruct").capabilities,
        Some(ModelCapabilities::local("qwen3-4b-instruct"))
    );
    assert_eq!(
        get(&db, "qwen3-4b-instruct")
            .capabilities
            .and_then(|c| c.reasoning),
        Some(ReasoningControl::ModelManaged)
    );
    assert_eq!(
        get(&db, "qwen2.5-0.5b")
            .capabilities
            .and_then(|c| c.reasoning),
        Some(ReasoningControl::Unavailable)
    );
    assert_eq!(get(&db, "already-known").capabilities, Some(already));
    assert_eq!(
        get(&db, &remote_id).capabilities,
        None,
        "a provider row's NULL means not refreshed yet and must not be filled"
    );

    let second = db
        .write(|tx| Ok(models_store::backfill_local_capabilities(tx, local).expect("backfill")))
        .expect("second backfill");
    assert_eq!(second, 0, "a second pass changes nothing");
}

fn set_tool_use(db: &Database, id: &str, tool_use: ToolUse) -> bool {
    let mut changed = false;
    db.write(|tx| {
        changed = models_store::set_tool_use(tx, id, tool_use).expect("set tool use");
        Ok(())
    })
    .expect("write");
    changed
}

#[test]
fn set_tool_use_replaces_only_that_field() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider = Uuid::new_v4();
    let id = format!("{provider}:some-model");
    let mut record = claude_capabilities();
    record.tool_use = None;
    upsert(
        &db,
        &row(provider, &id, SourceKind::Provider, Some(record.clone())),
    );

    let found = ToolUse::new(ToolSupport::Unsupported, ToolUseBasis::Template);
    assert!(set_tool_use(&db, &id, found));

    let stored = get(&db, &id).capabilities.expect("record");
    assert_eq!(stored.tool_use, Some(found));
    assert_eq!(stored.reasoning, record.reasoning);
    assert_eq!(
        stored.accepted_attachment_kinds,
        record.accepted_attachment_kinds
    );
    assert_eq!(stored.thinking_style, record.thinking_style);
}

#[test]
fn set_tool_use_fills_a_row_with_no_record_and_ignores_a_missing_row() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider = Uuid::new_v4();
    let id = format!("{provider}:bare");
    upsert(&db, &row(provider, &id, SourceKind::Imported, None));
    let found = ToolUse::new(ToolSupport::Supported, ToolUseBasis::SelfTest);
    assert!(set_tool_use(&db, &id, found));
    assert_eq!(get(&db, &id).capabilities.unwrap().tool_use, Some(found));
    assert!(!set_tool_use(&db, "no-such-model", found));
}

#[test]
fn a_finding_of_lower_rank_never_overwrites_a_higher_one() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider = Uuid::new_v4();
    let id = format!("{provider}:ranked");
    upsert(&db, &row(provider, &id, SourceKind::Imported, None));

    let self_test = ToolUse::new(ToolSupport::Unsupported, ToolUseBasis::SelfTest);
    let template = ToolUse::new(ToolSupport::Unsupported, ToolUseBasis::Template);
    let curated = ToolUse::new(ToolSupport::Supported, ToolUseBasis::Curated);
    assert!(set_tool_use(&db, &id, self_test));
    assert!(set_tool_use(&db, &id, template));
    assert!(!set_tool_use(&db, &id, self_test), "downward is dropped");
    assert_eq!(get(&db, &id).capabilities.unwrap().tool_use, Some(template));
    assert!(set_tool_use(&db, &id, curated));
    assert!(!set_tool_use(&db, &id, template));
    assert_eq!(get(&db, &id).capabilities.unwrap().tool_use, Some(curated));
}

#[test]
fn installing_the_same_model_again_resets_the_finding() {
    let tmp = tempfile::tempdir().expect("tmp dir");
    let db = open_vault(tmp.path());
    let provider = Uuid::new_v4();
    let id = "my-own-finetune";
    upsert(
        &db,
        &row(
            provider,
            id,
            SourceKind::Imported,
            Some(ModelCapabilities::local(id)),
        ),
    );
    set_tool_use(
        &db,
        id,
        ToolUse::new(ToolSupport::Unsupported, ToolUseBasis::Template),
    );
    // A new file under the same id is installed: the record is written again from scratch.
    upsert(
        &db,
        &row(
            provider,
            id,
            SourceKind::Imported,
            Some(ModelCapabilities::local(id)),
        ),
    );
    assert_eq!(get(&db, id).capabilities.unwrap().tool_use, None);
}

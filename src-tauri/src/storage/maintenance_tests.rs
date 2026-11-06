//! Tests for `maintenance` (spec 022-session-restore, research R5/R6,
//! contracts/wm-session.md §2). They open a real vault: the legacy
//! preference delete needs `current_hlc()` and the maintenance table only
//! exists after holzi's migrations.

// These tests seed and inspect raw vault state that the CRDT write path does
// not expose.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use uuid::Uuid;

use super::maintenance::{
    fold_scoped_preferences, prune_delete_markers, run_after_open, run_pending_tasks,
    DELETE_MARKER_RETENTION_DAYS, LEGACY_ACTIVE_WORKSPACE_KEY, VACUUM_TASK,
};
use super::preferences::{self, PrefScope};
use crate::identity::{
    holzi_migration_source, installation_id_path, read_or_mint_installation_uuid, HolziBootstrap,
    HOLZI_TRIGGER_VERSION,
};
use crate::providers::local::ensure_local_provider;
use crate::storage::known_devices;
use crate::storage::models::{self, IntegrityStatus, ModelRow, SourceKind};
use crate::storage::query;

fn open_test_vault(passphrase: &str) -> (tempfile::TempDir, Database, Uuid) {
    let dir = tempfile::tempdir().expect("tempdir");
    let install_path = installation_id_path(dir.path());
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new(passphrase),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(install_path.clone())),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open test vault");
    let installation_uuid =
        read_or_mint_installation_uuid(&install_path).expect("installation uuid");
    let device = query::read(&db, |r| {
        known_devices::get_vault_device_uuid(r, installation_uuid)
    })
    .expect("read vault_device_uuid")
    .expect("bootstrap registered this installation");
    (dir, db, device)
}

fn query_i64(db: &Database, sql: &str) -> i64 {
    db.with_connection(|conn| Ok(conn.query_row(sql, [], |r| r.get(0))?))
        .expect("query")
}

fn pending_vacuum(db: &Database) -> bool {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM holzi_maintenance_no_sync WHERE task = ?1",
            params![VACUUM_TASK],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .expect("read maintenance")
        > 0
}

#[test]
fn run_after_open_turns_on_secure_delete() {
    let (_dir, db, _device) = open_test_vault("maintenance-secure-delete");
    run_after_open(&db);
    assert_eq!(query_i64(&db, "PRAGMA secure_delete"), 1);
}

#[test]
fn run_after_open_deletes_the_legacy_active_workspace_preference_everywhere() {
    let (_dir, db, device) = open_test_vault("maintenance-legacy-pref");
    db.write(|tx| {
        preferences::insert_or_update(
            tx,
            PrefScope::Device(device),
            LEGACY_ACTIVE_WORKSPACE_KEY,
            "ws-1",
        )?;
        preferences::insert_or_update(tx, PrefScope::Vault, LEGACY_ACTIVE_WORKSPACE_KEY, "ws-2")?;
        preferences::insert_or_update(tx, PrefScope::Device(device), "voice.auto_send", "true")?;
        Ok(())
    })
    .expect("write preferences");

    run_after_open(&db);

    let legacy = query::read(&db, |r| {
        preferences::list_by_key(r, LEGACY_ACTIVE_WORKSPACE_KEY)
    })
    .expect("list legacy");
    assert!(legacy.is_empty());
    let kept = query::read(&db, |r| {
        preferences::get(r, PrefScope::Device(device), "voice.auto_send")
    })
    .expect("read kept");
    assert_eq!(kept.as_deref(), Some("true"));
}

#[test]
fn run_after_open_backfills_model_metadata() {
    let (_dir, db, _device) = open_test_vault("maintenance-model-backfills");
    let entry = crate::catalog::entries()
        .first()
        .expect("the built-in catalog is not empty");
    let local_provider_id = db
        .write(|tx| {
            let provider_id = ensure_local_provider(tx)?;
            models::upsert_model(
                tx,
                &ModelRow {
                    id: entry.id.clone(),
                    provider_id,
                    name: entry.name.clone(),
                    context_window: Some(entry.context_window as i64),
                    fetched_at: None,
                    tokenizer_repo: None,
                    hf_repo: None,
                    hf_filename: None,
                    hf_revision: None,
                    hf_revision_ref: None,
                    file_sha256: None,
                    integrity_status: IntegrityStatus::Unknown,
                    source_kind: SourceKind::Provider,
                    capabilities: None,
                },
            )?;
            Ok(provider_id)
        })
        .expect("seed model metadata");

    run_after_open(&db);

    let model = query::read(&db, |r| models::get_model(r, &entry.id))
        .expect("read backfilled model")
        .expect("seeded model");
    assert_eq!(model.provider_id, local_provider_id);
    assert_eq!(
        model.tokenizer_repo.as_deref(),
        Some(entry.tokenizer_repo.as_str())
    );
    assert_eq!(model.source_kind, SourceKind::Catalog);
    assert!(model.capabilities.is_some());
}

#[test]
fn the_queued_vacuum_runs_once_and_leaves_no_free_pages() {
    let (_dir, db, _device) = open_test_vault("maintenance-vacuum");
    assert!(pending_vacuum(&db), "migration 0020 queues the VACUUM");

    run_after_open(&db);
    assert!(!pending_vacuum(&db));
    assert_eq!(query_i64(&db, "PRAGMA freelist_count"), 0);

    // A second run finds nothing to do.
    run_after_open(&db);
    assert!(!pending_vacuum(&db));
}

#[test]
fn a_failing_vacuum_keeps_the_task_for_the_next_open() {
    let (_dir, db, _device) = open_test_vault("maintenance-vacuum-fails");
    // VACUUM cannot run inside a transaction: this makes the task fail.
    let result = db.with_connection(|conn| {
        let tx = conn.unchecked_transaction()?;
        let outcome = run_pending_tasks(&tx);
        tx.rollback()?;
        Ok(outcome)
    });
    assert!(result.expect("run in a transaction").is_err());
    assert!(pending_vacuum(&db), "the task stays queued");

    run_after_open(&db);
    assert!(!pending_vacuum(&db), "the next open catches up");
}

fn set(db: &Database, scope: PrefScope, key: &str, value: &str) {
    db.write(|tx| preferences::insert_or_update(tx, scope, key, value))
        .expect("set preference");
}

fn get(db: &Database, scope: PrefScope, key: &str) -> Option<String> {
    query::read(db, |r| preferences::get(r, scope, key)).expect("get preference")
}

fn fold(db: &Database, device: Uuid) {
    db.write(|tx| fold_scoped_preferences(tx, device))
        .expect("fold preferences");
}

#[test]
fn a_device_value_of_a_vault_setting_becomes_the_vault_value() {
    let (_dir, db, device) = open_test_vault("fold-vault-settings");
    set(
        &db,
        PrefScope::Device(device),
        "chat.autonomy_mode",
        "ungated",
    );
    set(
        &db,
        PrefScope::Device(device),
        "cli_delegate.deny_rules",
        "[\"git_push\"]",
    );
    set(
        &db,
        PrefScope::Device(device),
        "chat.reasoning_option.qwen",
        "high",
    );

    fold(&db, device);

    for (key, value) in [
        ("chat.autonomy_mode", "ungated"),
        ("cli_delegate.deny_rules", "[\"git_push\"]"),
        ("chat.reasoning_option.qwen", "high"),
    ] {
        assert_eq!(
            get(&db, PrefScope::Vault, key).as_deref(),
            Some(value),
            "{key}"
        );
        assert_eq!(get(&db, PrefScope::Device(device), key), None, "{key}");
    }
}

#[test]
fn an_existing_vault_value_wins_and_the_device_value_is_dropped() {
    let (_dir, db, device) = open_test_vault("fold-vault-wins");
    set(&db, PrefScope::Vault, "wm.session_restore", "false");
    set(&db, PrefScope::Device(device), "wm.session_restore", "true");

    fold(&db, device);

    assert_eq!(
        get(&db, PrefScope::Vault, "wm.session_restore").as_deref(),
        Some("false")
    );
    assert_eq!(
        get(&db, PrefScope::Device(device), "wm.session_restore"),
        None
    );
}

#[test]
fn a_vault_value_of_a_device_setting_becomes_this_devices_when_it_has_none() {
    let (_dir, db, device) = open_test_vault("fold-device-setting");
    set(&db, PrefScope::Vault, "chat.notifications", "off");

    fold(&db, device);

    assert_eq!(
        get(&db, PrefScope::Device(device), "chat.notifications").as_deref(),
        Some("off")
    );
    assert_eq!(get(&db, PrefScope::Vault, "chat.notifications"), None);

    set(&db, PrefScope::Vault, "chat.notifications", "on");
    fold(&db, device);
    assert_eq!(
        get(&db, PrefScope::Device(device), "chat.notifications").as_deref(),
        Some("off"),
        "this device keeps its own value"
    );
    assert_eq!(get(&db, PrefScope::Vault, "chat.notifications"), None);
}

#[test]
fn folding_leaves_device_settings_alone_and_runs_on_open() {
    let (_dir, db, device) = open_test_vault("fold-on-open");
    set(
        &db,
        PrefScope::Device(device),
        "voice.stt_model_id",
        "whisper-tiny",
    );
    set(
        &db,
        PrefScope::Device(device),
        "appearance.color_scheme",
        "dark",
    );

    run_after_open(&db);
    run_after_open(&db);

    assert_eq!(
        get(&db, PrefScope::Device(device), "voice.stt_model_id").as_deref(),
        Some("whisper-tiny")
    );
    assert_eq!(
        get(&db, PrefScope::Vault, "appearance.color_scheme").as_deref(),
        Some("dark")
    );
    assert_eq!(
        get(&db, PrefScope::Device(device), "appearance.color_scheme"),
        None
    );
}

#[test]
fn delete_markers_older_than_the_retention_are_pruned_and_newer_ones_stay() {
    let (_dir, db, _device) = open_test_vault("maintenance-prune-markers");
    set(&db, PrefScope::Vault, "chat.permission_mode", "auto");
    db.write(|tx| preferences::delete(tx, PrefScope::Vault, "chat.permission_mode"))
        .expect("delete leaves a marker");

    // A copy of that marker, dated one day past the retention. The time part of an HLC is an
    // NTP64 timestamp: seconds in the upper 32 bits.
    let age = (i64::from(DELETE_MARKER_RETENTION_DAYS + 1) * 86_400) << 32;
    db.with_connection(|conn| {
        conn.execute(
            "INSERT INTO haex_deleted_rows (id, table_name, row_pks, haex_hlc_no_sync) \
             SELECT 'old-marker', table_name, row_pks, \
               (CAST(substr(haex_hlc_no_sync, 1, instr(haex_hlc_no_sync, '/') - 1) AS INTEGER) \
                 - ?1) || substr(haex_hlc_no_sync, instr(haex_hlc_no_sync, '/')) \
             FROM haex_deleted_rows WHERE table_name = 'preferences'",
            params![age],
        )?;
        Ok(())
    })
    .expect("backdate a copy of the marker");
    assert_eq!(query_i64(&db, "SELECT COUNT(*) FROM haex_deleted_rows"), 2);

    assert_eq!(prune_delete_markers(&db).expect("prune"), 1);
    assert_eq!(
        query_i64(
            &db,
            "SELECT COUNT(*) FROM haex_deleted_rows WHERE id = 'old-marker'"
        ),
        0,
        "the old marker is gone"
    );
    assert_eq!(
        query_i64(&db, "SELECT COUNT(*) FROM haex_deleted_rows"),
        1,
        "the recent marker stays"
    );
}

/// Spec 083 FR-024, T055: the local agent's two old settings are deleted for every device, a second
/// run changes nothing, and other preferences stay.
#[test]
fn run_after_open_deletes_the_local_agents_old_rights_on_every_device() {
    let (_dir, db, device) = open_test_vault("maintenance-local-agent-rights");
    db.write(|tx| {
        for scope in [PrefScope::Device(device), PrefScope::Vault] {
            preferences::insert_or_update(tx, scope, "agent_browser.builtin", "{}")?;
            preferences::insert_or_update(tx, scope, "agent_browser.builtin_passwords", "{}")?;
        }
        preferences::insert_or_update(tx, PrefScope::Device(device), "voice.auto_send", "true")?;
        Ok(())
    })
    .expect("write preferences");

    run_after_open(&db);
    for key in ["agent_browser.builtin", "agent_browser.builtin_passwords"] {
        let left = query::read(&db, |r| preferences::list_by_key(r, key)).expect("list");
        assert!(left.is_empty(), "{key} is gone on every device");
    }
    let again = db
        .write(crate::storage::maintenance::delete_local_agent_rights)
        .expect("a second run");
    assert_eq!(again, 0, "a second run changes nothing");
    let kept = query::read(&db, |r| {
        preferences::get(r, PrefScope::Device(device), "voice.auto_send")
    })
    .expect("read kept");
    assert_eq!(kept.as_deref(), Some("true"));
}

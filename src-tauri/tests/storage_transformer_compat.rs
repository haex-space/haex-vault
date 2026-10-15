//! Spec 024, T008 (research R19): every write of holzi's storage modules runs through haex-crdt's
//! CRDT transformer, which has to understand holzi's SQL. Each case writes through
//! `Database::write` against a freshly migrated vault and checks what sync relies on: the row
//! carries the transaction HLC, its column HLCs are filled and the table is marked dirty. A
//! delete leaves a delete marker instead. `_no_sync` tables pass through untouched.

// The checks read CRDT metadata columns and bookkeeping tables, which the CRDT write path does
// not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::{params, OptionalExtension, ToSql};
use haex_crdt::{hlc_is_newer, Database};
use uuid::Uuid;

use holzi_lib::chat::send_admission::{persist_send_transaction, PersistedSend};
use holzi_lib::identity::{installation_id_path, read_or_mint_installation_uuid};
use holzi_lib::instances::vault_config::vault_config;
use holzi_lib::model_capabilities::ModelCapabilities;
use holzi_lib::providers::local::ensure_local_provider;
use holzi_lib::storage::chat_messages::{self, ChatMessage, FinishReason, MessageRole};
use holzi_lib::storage::chat_threads::{self, ChatThread};
use holzi_lib::storage::known_devices;
use holzi_lib::storage::models::{self, IntegrityStatus, ModelRow, SourceKind};
use holzi_lib::storage::preferences::{self, PrefScope};
use holzi_lib::storage::providers::{self, Provider, ProviderCapability, ProviderKind};
use holzi_lib::storage::wm_session;

struct Vault {
    _dir: tempfile::TempDir,
    db: Database,
    installation_uuid: Uuid,
}

fn open_vault() -> Vault {
    let dir = tempfile::tempdir().expect("tempdir");
    let installation_id = installation_id_path(dir.path());
    let db = Database::open(vault_config(
        "storage-transformer-compat",
        &dir.path().join("vault.db"),
        &installation_id,
        true,
    ))
    .expect("open vault");
    let installation_uuid = read_or_mint_installation_uuid(&installation_id).expect("installation");
    clear_dirty(&db);
    Vault {
        _dir: dir,
        db,
        installation_uuid,
    }
}

fn clear_dirty(db: &Database) {
    db.with_connection(|conn| {
        conn.execute("DELETE FROM haex_crdt_dirty_tables_no_sync", [])?;
        Ok(())
    })
    .expect("clear dirty tables");
}

/// Asserts that the one row `table WHERE where_sql` carries a row HLC and column HLCs, that the
/// table is marked dirty, and returns the row HLC.
fn assert_synced(db: &Database, table: &str, where_sql: &str, args: &[&dyn ToSql]) -> String {
    let (hlc, column_hlcs) = crdt_meta(db, table, where_sql, args);
    assert!(
        column_hlcs.is_some_and(|map| map.len() > 2),
        "{table}: no column HLCs"
    );
    hlc
}

/// The row HLC and column-HLC map of the one row `table WHERE where_sql`, after asserting the
/// row HLC is set and the table is marked dirty.
fn crdt_meta(
    db: &Database,
    table: &str,
    where_sql: &str,
    args: &[&dyn ToSql],
) -> (String, Option<String>) {
    let (hlc, column_hlcs, dirty): (Option<String>, Option<String>, i64) = db
        .with_connection(|conn| {
            let (hlc, column_hlcs) = conn.query_row(
                &format!(
                    "SELECT haex_hlc_no_sync, haex_column_hlcs_no_sync FROM {table} \
                     WHERE {where_sql}"
                ),
                args,
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let dirty = conn.query_row(
                "SELECT COUNT(*) FROM haex_crdt_dirty_tables_no_sync WHERE table_name = ?1",
                params![table],
                |r| r.get(0),
            )?;
            Ok((hlc, column_hlcs, dirty))
        })
        .expect("read CRDT metadata");
    let hlc = hlc.unwrap_or_else(|| panic!("{table}: the transformer stamped no row HLC"));
    assert_eq!(dirty, 1, "{table}: not marked dirty");
    (hlc, column_hlcs)
}

fn assert_delete_marker(db: &Database, table: &str) {
    let hlc: Option<String> = db
        .with_connection(|conn| {
            Ok(conn
                .query_row(
                    "SELECT haex_hlc_no_sync FROM haex_deleted_rows WHERE table_name = ?1 \
                     AND haex_hlc_no_sync IS NOT NULL LIMIT 1",
                    params![table],
                    |r| r.get(0),
                )
                .optional()?)
        })
        .expect("read delete log");
    assert!(hlc.is_some(), "{table}: the delete left no delete marker");
}

fn row_count(db: &Database, table: &str) -> i64 {
    db.with_connection(|conn| {
        Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
    })
    .expect("count rows")
}

fn provider(kind: ProviderKind, adapter: Option<&str>) -> Provider {
    Provider {
        id: Uuid::new_v4(),
        kind,
        adapter: adapter.map(str::to_string),
        name: "provider".to_string(),
        base_url: None,
        credentials: Some(b"secret".to_vec()),
        created_at: 1,
        capability: ProviderCapability::Chat,
    }
}

fn model(id: &str, provider_id: Uuid) -> ModelRow {
    ModelRow {
        id: id.to_string(),
        provider_id,
        name: id.to_string(),
        context_window: Some(4096),
        fetched_at: Some(1),
        tokenizer_repo: None,
        hf_repo: None,
        hf_filename: None,
        hf_revision: None,
        hf_revision_ref: None,
        file_sha256: None,
        integrity_status: IntegrityStatus::Unknown,
        source_kind: SourceKind::Provider,
        capabilities: Some(ModelCapabilities::local(id)),
    }
}

fn thread(id: Uuid) -> ChatThread {
    ChatThread {
        id,
        title: "title".to_string(),
        last_provider_id: None,
        last_model_id: None,
        created_at: 1,
        updated_at: 1,
    }
}

fn message(thread_id: Uuid, parent_id: Option<Uuid>) -> ChatMessage {
    ChatMessage {
        id: Uuid::new_v4(),
        thread_id,
        parent_id,
        role: MessageRole::User,
        content: "hello".to_string(),
        provider_id: None,
        model_id: None,
        prompt_tokens: None,
        completion_tokens: None,
        finish_reason: Some(FinishReason::Complete),
        created_at: 5,
        idempotency_key: None,
        tool_name: None,
        tool_call_id: None,
        tool_input: None,
        tool_is_error: None,
        tool_source: None,
        autonomy_mode: None,
        tool_origin: None,
    }
}

#[test]
fn preference_insert_update_and_delete() {
    let v = open_vault();
    let scope = PrefScope::Vault;
    let key = "chat.permission_mode";
    let vault_uuid = scope.to_uuid().to_string();

    v.db.write(|tx| preferences::insert_or_update(tx, scope, key, "auto"))
        .expect("insert");
    let inserted = assert_synced(
        &v.db,
        "preferences",
        "vault_device_uuid = ?1 AND key = ?2",
        params![vault_uuid, key],
    );

    clear_dirty(&v.db);
    v.db.write(|tx| preferences::insert_or_update(tx, scope, key, "plan"))
        .expect("update");
    let updated = assert_synced(
        &v.db,
        "preferences",
        "vault_device_uuid = ?1 AND key = ?2",
        params![vault_uuid, key],
    );
    assert!(
        hlc_is_newer(&updated, &inserted),
        "the update carries a newer HLC"
    );

    v.db.write(|tx| preferences::delete(tx, scope, key))
        .expect("delete");
    assert_delete_marker(&v.db, "preferences");
}

#[test]
fn device_alias_update() {
    let v = open_vault();
    let installation = v.installation_uuid;
    // The bootstrap inserts this row before the HLC exists; `Database::open` stamps it
    // (haexmas/haex-crdt#38), so the update records per-column HLCs like any other write.
    v.db.write(|tx| known_devices::update_alias(tx, installation, "Laptop"))
        .expect("update alias");
    assert_synced(
        &v.db,
        "known_devices",
        "installation_uuid = ?1",
        params![installation.to_string()],
    );
}

#[test]
fn provider_insert_updates_and_delete() {
    let v = open_vault();
    let p = provider(ProviderKind::CliDelegate, Some("claude"));
    let id = p.id.to_string();

    v.db.write(|tx| providers::insert_provider(tx, &p))
        .expect("insert");
    assert_synced(&v.db, "providers", "id = ?1", params![id]);

    clear_dirty(&v.db);
    v.db.write(|tx| {
        providers::set_adapter(tx, p.id, "codex")?;
        providers::update_credentials(tx, p.id, b"rotated")
    })
    .expect("updates");
    assert_synced(&v.db, "providers", "id = ?1", params![id]);

    v.db.write(|tx| providers::delete_provider(tx, p.id))
        .expect("delete");
    assert_delete_marker(&v.db, "providers");
}

#[test]
fn local_provider_is_created_once() {
    let v = open_vault();
    let first = v.db.write(ensure_local_provider).expect("create");
    let second = v.db.write(ensure_local_provider).expect("reuse");
    assert_eq!(first, second);
    assert_synced(&v.db, "providers", "id = ?1", params![first.to_string()]);
}

#[test]
fn model_upsert_on_conflict_and_narrow_updates() {
    let v = open_vault();
    let local = v.db.write(ensure_local_provider).expect("local provider");

    v.db.write(|tx| models::upsert_model(tx, &model("qwen3-0.6b", local)))
        .expect("insert");
    let inserted = assert_synced(&v.db, "models", "id = ?1", params!["qwen3-0.6b"]);

    // Second upsert of the same id takes `ON CONFLICT … DO UPDATE`.
    clear_dirty(&v.db);
    let mut renamed = model("qwen3-0.6b", local);
    renamed.name = "Qwen 3".to_string();
    v.db.write(|tx| models::upsert_model(tx, &renamed))
        .expect("upsert");
    let upserted = assert_synced(&v.db, "models", "id = ?1", params!["qwen3-0.6b"]);
    assert!(
        hlc_is_newer(&upserted, &inserted),
        "DO UPDATE carries a newer HLC"
    );

    for (label, write) in [
        (
            "set_integrity_status",
            Box::new(|tx: &mut haex_crdt::CrdtTransaction<'_>| {
                models::set_integrity_status(tx, "qwen3-0.6b", IntegrityStatus::Verified)
            })
                as Box<dyn Fn(&mut haex_crdt::CrdtTransaction<'_>) -> haex_crdt::Result<usize>>,
        ),
        (
            "update_hf_revision_and_hash",
            Box::new(|tx| models::update_hf_revision_and_hash(tx, "qwen3-0.6b", "abc", "def")),
        ),
        (
            "backfill_source_kind",
            Box::new(move |tx| models::backfill_source_kind(tx, local, &["qwen3-0.6b"])),
        ),
    ] {
        clear_dirty(&v.db);
        let changed = v.db.write(|tx| write(tx)).expect(label);
        assert_eq!(changed, 1, "{label} changed the row");
        assert_synced(&v.db, "models", "id = ?1", params!["qwen3-0.6b"]);
    }
}

#[test]
fn model_backfills_and_cache_replacement() {
    let v = open_vault();
    let local = v.db.write(ensure_local_provider).expect("local provider");
    let mut bare = model("qwen3-0.6b", local);
    bare.capabilities = None;
    v.db.write(|tx| models::upsert_model(tx, &bare))
        .expect("seed");

    clear_dirty(&v.db);
    let filled =
        v.db.write(|tx| {
            Ok(
                models::backfill_tokenizer_repo(tx, &[("qwen3-0.6b", "org/tok")])?
                    + models::backfill_local_capabilities(tx, local)?,
            )
        })
        .expect("backfill");
    assert_eq!(filled, 2);
    assert_synced(&v.db, "models", "id = ?1", params!["qwen3-0.6b"]);

    let remote = provider(ProviderKind::ApiKey, Some("anthropic"));
    let remote_id = remote.id;
    v.db.write(|tx| providers::insert_provider(tx, &remote))
        .expect("remote provider");
    let first = format!("{remote_id}:opus");
    let stale = format!("{remote_id}:retired");
    v.db.write(|tx| {
        models::replace_provider_models(
            tx,
            remote_id,
            &[model(&first, remote_id), model(&stale, remote_id)],
        )
    })
    .expect("first refresh");
    v.db.write(|tx| models::replace_provider_models(tx, remote_id, &[model(&first, remote_id)]))
        .expect("second refresh drops the stale row");
    assert_synced(&v.db, "models", "id = ?1", params![first]);
    assert_delete_marker(&v.db, "models");
}

#[test]
fn chat_thread_and_message_writes() {
    let v = open_vault();
    let id = Uuid::new_v4();
    let first = message(id, None);
    let first_id = first.id;

    v.db.write(|tx| {
        chat_threads::insert_thread(tx, &thread(id))?;
        chat_messages::insert_message(tx, &first)
    })
    .expect("insert thread and message");
    let thread_hlc = assert_synced(&v.db, "chat_threads", "id = ?1", params![id.to_string()]);
    let message_hlc = assert_synced(
        &v.db,
        "chat_messages",
        "id = ?1",
        params![first_id.to_string()],
    );
    assert_eq!(
        thread_hlc, message_hlc,
        "one write is one transaction group"
    );

    // The child's `created_at` comes from a subselect in `VALUES`.
    let child = message(id, Some(first_id));
    let child_id = child.id;
    v.db.write(|tx| chat_messages::insert_message(tx, &child))
        .expect("insert child");
    assert_synced(
        &v.db,
        "chat_messages",
        "id = ?1",
        params![child_id.to_string()],
    );

    clear_dirty(&v.db);
    v.db.write(|tx| {
        chat_threads::rename_title(tx, id, "renamed")?;
        chat_threads::update_thread(tx, id, "renamed", None, Some("model"), 9)
    })
    .expect("rename and update");
    assert_synced(&v.db, "chat_threads", "id = ?1", params![id.to_string()]);

    v.db.write(|tx| chat_messages::delete_message(tx, child_id))
        .expect("delete message");
    assert_delete_marker(&v.db, "chat_messages");

    assert!(v
        .db
        .write(|tx| chat_threads::delete_thread_and_messages(tx, id))
        .expect("delete thread"));
    assert_delete_marker(&v.db, "chat_threads");
    assert_eq!(row_count(&v.db, "chat_messages"), 0);
}

#[test]
fn a_send_is_one_transaction_group() {
    let v = open_vault();
    let sent =
        v.db.write(|tx| persist_send_transaction(tx, "compat-key", None, "hi", None, "model", 3))
            .expect("persist send");
    let PersistedSend::Fresh {
        thread_id,
        user_message_id,
        ..
    } = sent
    else {
        panic!("expected a fresh send, got {sent:?}");
    };
    let thread_hlc = assert_synced(
        &v.db,
        "chat_threads",
        "id = ?1",
        params![thread_id.to_string()],
    );
    let message_hlc = assert_synced(
        &v.db,
        "chat_messages",
        "id = ?1",
        params![user_message_id.to_string()],
    );
    assert_eq!(thread_hlc, message_hlc);
}

#[test]
fn a_no_sync_table_is_written_without_crdt_bookkeeping() {
    let v = open_vault();
    let device = Uuid::new_v4();
    v.db.write(|tx| {
        wm_session::save(tx, device, &serde_json::json!({ "version": 1 }))
            .map_err(haex_crdt::Error::consumer)
    })
    .expect("save session");
    v.db.write(|tx| {
        wm_session::save(tx, device, &serde_json::json!({ "version": 2 }))
            .map_err(haex_crdt::Error::consumer)
    })
    .expect("save again through ON CONFLICT");
    assert_eq!(row_count(&v.db, "wm_sessions_no_sync"), 1);
    assert_eq!(row_count(&v.db, "haex_crdt_dirty_tables_no_sync"), 0);

    v.db.write(|tx| wm_session::delete(tx, device))
        .expect("delete session");
    let markers: i64 =
        v.db.with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'wm_sessions_no_sync'",
                [],
                |r| r.get(0),
            )?)
        })
        .expect("count delete markers");
    assert_eq!(markers, 0, "a device-local delete leaves no delete marker");
}

#[test]
fn the_sync_setup_writes_crdt_rows() {
    let v = open_vault();
    let state = holzi_lib::sync::genesis::ensure_sync_state(&v.db, v.installation_uuid, true)
        .expect("sync state");
    let device = state.device_pubkey.expect("device key");

    let identity = assert_synced(&v.db, "vault_identity", "id = 1", &[]);
    let list = assert_synced(&v.db, "device_lists", "generation = 1", &[]);
    let generation = assert_synced(&v.db, "vault_key_generations", "generation = 1", &[]);
    let envelope = assert_synced(
        &v.db,
        "vault_key_envelopes",
        "recipient = ?1",
        params![device.as_slice()],
    );
    assert!(
        identity == list && list == generation && generation == envelope,
        "the whole setup is one transaction group"
    );
    assert_eq!(row_count(&v.db, "device_keys_no_sync"), 1);
    assert_eq!(row_count(&v.db, "vault_content_keys_no_sync"), 1);
}

//! Migration tests for `0020_wm_session_no_sync` (spec 022-session-restore,
//! research R5), `0021_own_device_sync` (spec 024, research R2/R3) and `0022_passwords` (spec 034,
//! data-model.md). Each
//! case runs against a genesis vault (every migration applied at
//! `Database::open`) and against a vault upgraded from the previous schema,
//! where the migration has to carry existing rows along.

// These tests build legacy schemas and read `sqlite_master` and the delete log
// directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::{
    Database, DatabaseConfig, MigrationName, NoopSignatureProvider, SqlCipherKey,
    StaticMigrationSource,
};
use uuid::Uuid;

use crate::identity::{
    holzi_migration_source, installation_id_path, read_or_mint_installation_uuid, HolziBootstrap,
    HOLZI_TRIGGER_VERSION,
};
use crate::storage::known_devices;

const LEGACY_TABLES: [&str; 3] = ["workspaces", "shell_windows", "shell_window_tabs"];

/// The frozen migration set, truncated to everything before `name` — models
/// "a vault provisioned by an older holzi build that hasn't seen this
/// migration yet".
pub(super) fn migration_source_before(name: &str) -> Arc<StaticMigrationSource> {
    let full = holzi_migration_source();
    let filtered: BTreeMap<MigrationName, String> = full
        .0
        .iter()
        .filter(|(k, _)| k.as_str() < name)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Arc::new(StaticMigrationSource(filtered))
}

pub(super) fn open(
    dir: &Path,
    passphrase: &str,
    create: bool,
    source: Arc<StaticMigrationSource>,
) -> Database {
    Database::open(DatabaseConfig {
        path: dir.join("vault.db"),
        key: SqlCipherKey::new(passphrase),
        create_if_missing: create,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: source,
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the test vault")
}

pub(super) fn device_of(db: &Database, dir: &Path) -> Uuid {
    let installation_uuid =
        read_or_mint_installation_uuid(&installation_id_path(dir)).expect("installation uuid");
    crate::storage::query::read(db, |r| {
        known_devices::get_vault_device_uuid(r, installation_uuid)
    })
    .expect("read vault_device_uuid")
    .expect("bootstrap registered this installation")
}

pub(super) fn table_exists(db: &Database, name: &str) -> bool {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .expect("query sqlite_master")
        > 0
}

fn maintenance_tasks(db: &Database) -> Vec<String> {
    db.with_connection(|conn| {
        let mut stmt = conn.prepare("SELECT task FROM holzi_maintenance_no_sync ORDER BY task")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    })
    .expect("read maintenance tasks")
}

fn delete_markers_for_legacy_tables(db: &Database) -> i64 {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM haex_deleted_rows \
             WHERE table_name IN ('workspaces', 'shell_windows', 'shell_window_tabs')",
            [],
            |r| r.get(0),
        )?)
    })
    .expect("count delete markers")
}

fn assert_current_schema(db: &Database) {
    for table in LEGACY_TABLES {
        assert!(!table_exists(db, table), "{table} must be dropped by 0020");
    }
    assert!(table_exists(db, "wm_sessions_no_sync"));
    assert!(table_exists(db, "holzi_maintenance_no_sync"));
    assert_eq!(
        maintenance_tasks(db),
        vec!["vacuum_after_legacy_wm_drop".to_string()],
        "0020 queues the one-time VACUUM (research R6)"
    );
}

#[tokio::test]
async fn migration_0020_gives_a_fresh_vault_the_session_tables_only() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "wm-session-migration-fresh",
            true,
            holzi_migration_source(),
        );
        assert_current_schema(&db);
        assert_eq!(delete_markers_for_legacy_tables(&db), 0);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0020_drops_populated_spec_015_tables_without_delete_markers() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");

        // First open: the schema before 0020, with a saved spec 015 session.
        {
            let db = open(
                dir.path(),
                "wm-session-migration-upgrade",
                true,
                migration_source_before("0020_wm_session_no_sync"),
            );
            let device = device_of(&db, dir.path()).to_string();
            db.with_connection(|conn| {
                conn.execute(
                    "INSERT INTO workspaces (vault_device_uuid, workspace_id, position, haex_hlc_no_sync) \
                     VALUES (?1, 'ws-1', 0, current_hlc())",
                    params![device],
                )?;
                conn.execute(
                    "INSERT INTO shell_windows \
                     (vault_device_uuid, window_id, workspace_id, x, y, width, height, \
                      is_minimized, is_maximized, stack_order, active_tab_id, haex_hlc_no_sync) \
                     VALUES (?1, 'win-1', 'ws-1', 0, 0, 800, 600, 0, 0, 1, 'tab-1', current_hlc())",
                    params![device],
                )?;
                conn.execute(
                    "INSERT INTO shell_window_tabs \
                     (vault_device_uuid, tab_id, window_id, app_id, position, haex_hlc_no_sync) \
                     VALUES (?1, 'tab-1', 'win-1', 'system.chat', 0, current_hlc())",
                    params![device],
                )?;
                Ok(())
            })
            .expect("save a spec 015 session");
            assert_eq!(delete_markers_for_legacy_tables(&db), 0);
        }

        // Second open: the current build applies 0020 on top.
        let db = open(
            dir.path(),
            "wm-session-migration-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_current_schema(&db);
        assert_eq!(
            delete_markers_for_legacy_tables(&db),
            0,
            "DROP TABLE must not write delete markers that would sync to other devices"
        );
    })
    .await
    .expect("join");
}

const SYNC_TABLES: [&str; 10] = [
    "vault_identity_secret_no_sync",
    "device_keys_no_sync",
    "device_lists",
    "vault_key_generations",
    "vault_key_envelopes",
    "vault_content_keys_no_sync",
    "sync_progress_no_sync",
    "pending_links_no_sync",
    "admission_requests",
    "device_presence_no_sync",
];

pub(super) fn column_names(db: &Database, table: &str) -> Vec<String> {
    db.with_connection(|conn| {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")?;
        let rows = stmt.query_map(params![table], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    })
    .expect("read columns")
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|conn| Ok(conn.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn stored_seed(db: &Database) -> Option<Vec<u8>> {
    use haex_crdt::rusqlite::OptionalExtension;
    db.with_connection(|conn| {
        Ok(conn
            .query_row(
                "SELECT privkey FROM vault_identity_secret_no_sync WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .optional()?)
    })
    .expect("read seed")
}

/// Opens a vault on the schema before 0021 and gives it the placeholder
/// identity the bootstrap minted before spec 024.
fn legacy_vault_with_placeholder(dir: &Path, passphrase: &str, placeholder: [u8; 32]) {
    let db = open(
        dir,
        passphrase,
        true,
        migration_source_before("0021_own_device_sync"),
    );
    db.with_connection(|conn| {
        conn.execute(
            "INSERT INTO vault_identity (id, pubkey, privkey) VALUES (1, ?1, ?2)",
            params![[3u8; 33].as_slice(), placeholder.as_slice()],
        )?;
        Ok(())
    })
    .expect("placeholder identity");
}

#[tokio::test]
async fn migration_0021_gives_a_fresh_vault_the_sync_tables_and_an_empty_identity() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "sync-migration-fresh",
            true,
            holzi_migration_source(),
        );
        for table in SYNC_TABLES {
            assert!(table_exists(&db, table), "{table} is created by 0021");
        }
        let columns = column_names(&db, "vault_identity");
        assert!(columns.contains(&"pubkey".to_string()));
        assert!(!columns.contains(&"privkey".to_string()), "{columns:?}");
        assert!(
            columns.contains(&"haex_hlc_no_sync".to_string()),
            "the rebuilt table stays a CRDT table"
        );
        assert_eq!(count(&db, "SELECT COUNT(*) FROM vault_identity"), 0);
        assert_eq!(stored_seed(&db), None);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0021_keeps_the_placeholder_as_seed_without_delete_markers() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let placeholder = [11u8; 32];
        legacy_vault_with_placeholder(dir.path(), "sync-migration-upgrade", placeholder);

        let db = open(
            dir.path(),
            "sync-migration-upgrade",
            false,
            holzi_migration_source(),
        );

        assert_eq!(stored_seed(&db), Some(placeholder.to_vec()));
        assert_eq!(count(&db, "SELECT COUNT(*) FROM vault_identity"), 0);
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'vault_identity'"
            ),
            0,
            "rebuilding the identity table writes no delete marker"
        );
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn two_copies_of_a_legacy_vault_derive_the_same_identity() {
    tokio::task::spawn_blocking(|| {
        let first_dir = tempfile::tempdir().expect("tempdir");
        let second_dir = tempfile::tempdir().expect("tempdir");
        let placeholder = [12u8; 32];
        legacy_vault_with_placeholder(first_dir.path(), "sync-migration-copies", placeholder);
        std::fs::copy(
            first_dir.path().join("vault.db"),
            second_dir.path().join("vault.db"),
        )
        .expect("copy the vault file");

        let identity = |dir: &Path| {
            let db = open(
                dir,
                "sync-migration-copies",
                false,
                holzi_migration_source(),
            );
            let installation =
                read_or_mint_installation_uuid(&installation_id_path(dir)).expect("installation");
            crate::sync::genesis::ensure_sync_state(&db, installation, false)
                .expect("sync state")
                .vault_pubkey
                .expect("the placeholder yields an identity")
        };
        let first = identity(first_dir.path());
        let second = identity(second_dir.path());

        let derived = crate::sync::keys::derive_vault_identity(&placeholder);
        assert_eq!(
            first, second,
            "research R3: every copy derives the same identity"
        );
        assert_eq!(
            first,
            crate::sync::signing::xonly_public_key(&derived).expect("public key")
        );
    })
    .await
    .expect("join");
}

const PASSWORDS_TABLES: [(&str, &[&str]); 12] = [
    (
        "haex_passwords_item_details",
        &[
            "id",
            "title",
            "username",
            "password",
            "note",
            "icon",
            "color",
            "url",
            "otp_secret",
            "otp_digits",
            "otp_period",
            "otp_algorithm",
            "expires_at",
            "autofill_aliases",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "haex_passwords_item_key_values",
        &["id", "item_id", "key", "value", "updated_at"],
    ),
    (
        "haex_passwords_groups",
        &[
            "id",
            "name",
            "description",
            "icon",
            "sort_order",
            "color",
            "parent_id",
            "created_at",
            "updated_at",
            "trashed_from_parent_id",
        ],
    ),
    (
        "haex_passwords_group_items",
        &["item_id", "group_id", "trashed_from_group_id"],
    ),
    (
        "haex_passwords_binaries",
        &["hash", "data", "size", "type", "created_at", "orphaned_at"],
    ),
    (
        "haex_passwords_item_binaries",
        &["id", "item_id", "binary_hash", "file_name"],
    ),
    (
        "haex_passwords_item_snapshots",
        &[
            "id",
            "item_id",
            "snapshot_data",
            "created_at",
            "modified_at",
        ],
    ),
    (
        "haex_passwords_snapshot_binaries",
        &["id", "snapshot_id", "binary_hash", "file_name"],
    ),
    (
        "haex_passwords_generator_presets",
        &[
            "id",
            "name",
            "length",
            "uppercase",
            "lowercase",
            "numbers",
            "symbols",
            "exclude_chars",
            "use_pattern",
            "pattern",
            "is_default",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "haex_passwords_tags",
        &["id", "name", "color", "created_at"],
    ),
    ("haex_passwords_item_tags", &["id", "item_id", "tag_id"]),
    (
        "haex_passwords_passkeys",
        &[
            "id",
            "item_id",
            "credential_id",
            "relying_party_id",
            "relying_party_name",
            "user_name",
            "user_display_name",
            "user_handle",
            "private_key",
            "public_key",
            "algorithm",
            "sign_count",
            "is_discoverable",
            "icon",
            "color",
            "nickname",
            "created_at",
            "last_used_at",
        ],
    ),
];

/// The columns of `table` without the metadata haex-crdt adds to every tracked table.
pub(super) fn own_columns(db: &Database, table: &str) -> Vec<String> {
    column_names(db, table)
        .into_iter()
        .filter(|name| !name.starts_with("haex_"))
        .collect()
}

fn assert_passwords_schema(db: &Database) {
    for (table, columns) in PASSWORDS_TABLES {
        assert!(table_exists(db, table), "{table} must exist after 0022");
        // Later migrations append columns (0027 `owner`), so 0022's columns are the leading ones.
        let own = own_columns(db, table);
        assert_eq!(
            &own[..columns.len().min(own.len())],
            columns,
            "columns of {table}"
        );
    }
    let declared_type = db
        .with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT type FROM pragma_table_info('haex_passwords_binaries') WHERE name = 'data'",
                [],
                |r| r.get::<_, String>(0),
            )?)
        })
        .expect("declared type of the binary column");
    assert_eq!(
        declared_type, "BLOB",
        "attachments are binary, not Base64 text (A1)"
    );
    for (table, _) in PASSWORDS_TABLES {
        let unique_indexes = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM pragma_index_list(?1) \
                     WHERE \"unique\" = 1 AND origin IN ('c', 'u')",
                    params![table],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .expect("index list");
        assert_eq!(
            unique_indexes, 0,
            "{table}: a UNIQUE constraint would halt the sync on a conflict (A3)"
        );
    }
}

#[tokio::test]
async fn migration_0022_gives_a_fresh_vault_the_password_tables() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "passwords-migration-fresh",
            true,
            holzi_migration_source(),
        );
        assert_passwords_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0022_upgrades_a_vault_from_before_it() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "passwords-migration-upgrade",
            true,
            migration_source_before("0022_passwords"),
        );
        assert!(!table_exists(&old, "haex_passwords_item_details"));
        drop(old);

        let db = open(
            dir.path(),
            "passwords-migration-upgrade",
            false,
            holzi_migration_source(),
        );
        assert_passwords_schema(&db);
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn a_binary_that_an_attachment_still_links_cannot_be_deleted() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "passwords-migration-restrict",
            true,
            holzi_migration_source(),
        );
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_passwords_item_details (id) VALUES ('item-1')",
                [],
            )?;
            conn.execute(
                "INSERT INTO haex_passwords_binaries (hash, data, size) VALUES ('h1', x'0102', 2)",
                [],
            )?;
            conn.execute(
                "INSERT INTO haex_passwords_item_binaries (id, item_id, binary_hash, file_name) \
                 VALUES ('l1', 'item-1', 'h1', 'a.txt')",
                [],
            )?;
            let refused = conn.execute("DELETE FROM haex_passwords_binaries WHERE hash = 'h1'", []);
            assert!(
                refused.is_err(),
                "ON DELETE RESTRICT must refuse deleting a linked binary (A4)"
            );
            // Without the link the binary can go.
            conn.execute(
                "DELETE FROM haex_passwords_item_binaries WHERE id = 'l1'",
                [],
            )?;
            conn.execute("DELETE FROM haex_passwords_binaries WHERE hash = 'h1'", [])?;
            Ok(())
        })
        .expect("restrict test");
    })
    .await
    .expect("join");
}

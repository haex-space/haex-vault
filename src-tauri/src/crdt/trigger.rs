// src-tauri/src/crdt/trigger.rs
//
// Vault's shared-space trigger layer, composed on top of `haex_crdt`'s
// generic CRDT trigger installer.
//
// Per design decision D-3 the crate stays space-agnostic: it installs the
// INSERT / UPDATE / BEFORE-DELETE triggers that maintain the per-column HLC
// map, append delete events to `haex_deleted_rows`, and mark dirty tables.
// Everything that knows about spaces — MLS group state, UCAN authorization,
// the per-space delete-log — is vault's, and lives here.
//
// The composition is deliberately two-layered: `haex_crdt` owns its own DDL,
// this module owns vault's. `install_crdt_with_shared_space` /
// `drop_crdt_with_shared_space` are the entry points that stack them.
//
// The re-export block below is a transitional shim: ~30 call sites across the
// app still `use crate::crdt::trigger::{...}` for the crate's generic surface.
// A later batch flattens them to `haex_crdt` imports.

use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};
use rusqlite::{Connection, Transaction};

pub use haex_crdt::crdt::columns::{
    COLUMN_HLCS_COLUMN, COLUMN_SIGS_COLUMN, DELETED_ROWS_TABLE, HLC_FUNCTION_NAME,
    HLC_TIMESTAMP_COLUMN, UUID_FUNCTION_NAME,
};
pub use haex_crdt::{
    ensure_crdt_columns, get_table_schema, is_safe_identifier, ColumnInfo, CrdtSetupError,
    TriggerSetupResult,
};

/// Name des Registers (`haex_shared_space_sync`) — die per-Space-Zuordnung
/// business_row → space. DELETE auf dieser Tabelle fächert per Fanout-Trigger
/// zusätzlich in `haex_shared_space_deleted_rows` (ADR 0002 §6.5).
pub const SHARED_SPACE_SYNC_TABLE: &str = "haex_shared_space_sync";

/// Name des per-Space Delete-Logs (ADR 0002 §6.5, revised 2026-07-29). Anders
/// als `haex_deleted_rows` (Owner-Domain) trägt jede Zeile hier explizit die
/// Space-Zugehörigkeit, damit Applying-Members den Empfänger-Reduce ausführen
/// können (Row + Register löschen) — vgl. Task 6.
pub const SHARED_SPACE_DELETED_ROWS_TABLE: &str = "haex_shared_space_deleted_rows";

/// Name der Register-DELETE-Fanout-Trigger. Zweiter Trigger neben dem generischen
/// `z_dirty_haex_shared_space_sync_delete`; feuert zusätzlich das per-Space
/// Signal in `haex_shared_space_deleted_rows`.
const SHARED_SPACE_DELETE_FANOUT_TRIGGER_TPL: &str = "z_shared_space_delete_fanout";

/// Trigger-Namensschema für Task 5 Path A (Direct-Emit auf Space-Scoped Infra
/// Tabellen). Jede der 5 Infra-Tabellen bekommt einen dedizierten Trigger,
/// der das per-Space Signal aus OLD.space_id direkt schreibt.
const SHARED_SPACE_INFRA_EMIT_TRIGGER_TPL: &str = "z_shared_space_infra_emit_{TABLE_NAME}_delete";

/// Trigger-Namensschema für Task 5 Path B (Register-Cascade auf Extension-
/// Tabellen). Jede CRDT-Tabelle, die als Register-Ziel legitim sein kann,
/// bekommt einen Trigger, der beim Hard-Delete die zugehörigen Register-
/// Zeilen löscht. Der Register-DELETE-Fanout (Task 4) übernimmt dann die
/// per-Space Fanout-Erzeugung.
const SHARED_SPACE_REGISTER_CASCADE_TRIGGER_TPL: &str =
    "z_shared_space_register_cascade_{TABLE_NAME}_delete";

/// Mirror of `haex_crdt`'s private `DELETE_TRIGGER_TPL`. The crate does not
/// export its trigger-name templates, and vault needs exactly one of them:
/// the generic BEFORE-DELETE trigger on
/// [`SHARED_SPACE_DELETED_ROWS_TABLE`] has to be suppressed after the crate
/// installs it (see [`add_shared_space_fanout`]). A crate-side rename would
/// make `install_on_shared_space_delete_log_suppresses_generic_delete_trigger`
/// fail rather than pass silently.
const CRATE_DELETE_TRIGGER_TPL: &str = "z_dirty_{TABLE_NAME}_delete";

/// Space-scoped Infra-Tabellen (Task 5 Path A): Trigger direct-emit.
/// Held in sync with `SPACE_SCOPED_CRDT_TABLES` minus the three infra-of-infra
/// tables (register, delete-log, anchor). Also mirrors
/// `REGISTER_TARGET_DENYLIST` for the same 5 entries — infra rows carry their
/// own `space_id` and are never register targets.
const SPACE_SCOPED_INFRA_TABLES: &[&str] = &[
    "haex_space_devices",
    "haex_space_members",
    "haex_peer_shares",
    "haex_mls_sync_keys",
    "haex_device_mls_enrollments",
];

/// Tabellen, für die Task 5 KEINEN Register-Cascade-Trigger anlegt.
/// Grund: Register selbst hat schon den Fanout (Task 4); die zwei Log-/Anchor-
/// Tabellen dürfen nicht cascaden (Retention-Pruning würde loopen).
const SHARED_SPACE_CASCADE_EXEMPT: &[&str] = &[
    "haex_shared_space_sync",
    "haex_shared_space_deleted_rows",
    "haex_space_compaction_anchors",
    "haex_deleted_rows",
];

/// Installs the generic CRDT triggers via [`haex_crdt::setup_triggers_for_table`]
/// and then layers vault's shared-space fan-out on top.
///
/// Each layer owns its own DDL (D-3): the crate installs INSERT / UPDATE /
/// BEFORE-DELETE, this function adds the per-space delete-log fanout, the
/// space-scoped infra direct-emit, and the register-cascade triggers.
///
/// `recreate` is forwarded to the crate (which drops its own three triggers
/// first) and additionally drops vault's shared-space triggers, so a recreate
/// leaves the DB in a fully clean state for both layers.
pub fn install_crdt_with_shared_space(
    tx: &Transaction,
    table_name: &str,
    recreate: bool,
) -> Result<TriggerSetupResult, CrdtSetupError> {
    let result = haex_crdt::setup_triggers_for_table(tx, table_name, recreate)?;

    if matches!(result, TriggerSetupResult::TableNotFound) {
        return Ok(result);
    }

    if recreate {
        // The crate already dropped its own three; vault's shared-space
        // triggers are re-created unconditionally below, so dropping them
        // here (rather than before the crate call) is equivalent and keeps
        // the crate's HLC/PK validation errors ahead of any DDL, exactly as
        // the pre-composition implementation did.
        drop_shared_space_triggers(tx, table_name)?;
    }

    let pks = primary_key_columns(tx, table_name)?;
    add_shared_space_fanout(tx, table_name, &pks)?;

    Ok(result)
}

/// Drops the generic CRDT triggers via [`haex_crdt::drop_triggers_for_table`]
/// and then vault's shared-space triggers for `table_name`.
///
/// Both layers drop unconditionally with `IF EXISTS`, so this is safe for a
/// table that never had the shared-space triggers installed.
pub fn drop_crdt_with_shared_space(
    tx: &Transaction,
    table_name: &str,
) -> Result<(), CrdtSetupError> {
    // The crate validates `table_name` as a safe identifier and errors before
    // emitting any SQL, which is what keeps `drop_shared_space_triggers`
    // (which interpolates the name) safe to call afterwards.
    haex_crdt::drop_triggers_for_table(tx, table_name)?;
    drop_shared_space_triggers(tx, table_name)
}

/// Ensures `table_name` has the CRDT columns, the crate's generic triggers,
/// and vault's shared-space fan-out. Returns `(columns_added, triggers_created)`.
///
/// **Probe semantics changed here on purpose.** Vault's pre-composition
/// version decided "triggers already exist" from a single probe for
/// `z_dirty_{table}_insert`. [`haex_crdt::ensure_crdt_columns_and_triggers`]
/// probes *every* trigger name it installs (exempting the delete trigger on
/// `haex_deleted_rows`), so a table that has the insert trigger but lost its
/// update or delete trigger is now repaired instead of skipped. That is
/// strictly more correct and is the behaviour we keep.
///
/// The shared-space layer is applied unconditionally rather than only when
/// the crate reports it created triggers — every generator uses
/// `CREATE TRIGGER IF NOT EXISTS`, so this is idempotent, and it closes the
/// same gap on vault's own layer (previously an extension table that already
/// had the insert trigger could never acquire its register-cascade trigger).
///
/// Note on [`SHARED_SPACE_DELETED_ROWS_TABLE`]: vault suppresses the crate's
/// generic delete trigger on that table, so the crate's probe would always
/// report it missing and re-run the install. Harmless (the install is
/// idempotent and the suppression re-applies) and unreachable in practice —
/// the only caller is `ensure_extension_tables_have_crdt`, which iterates
/// extension-owned tables.
pub fn ensure_crdt_columns_and_triggers(
    tx: &Transaction,
    table_name: &str,
) -> Result<(bool, bool), CrdtSetupError> {
    let (columns_added, triggers_created) =
        haex_crdt::ensure_crdt_columns_and_triggers(tx, table_name)?;

    // Empty PK list means the table does not exist (the crate errors on a
    // table that exists without a primary key), so there is nothing to layer
    // the shared-space triggers onto.
    let pks = primary_key_columns(tx, table_name)?;
    if !pks.is_empty() {
        add_shared_space_fanout(tx, table_name, &pks)?;
    }

    Ok((columns_added, triggers_created))
}

/// Vault's shared-space DDL for `table_name`, layered on top of a completed
/// [`haex_crdt::setup_triggers_for_table`].
///
/// Gate conditions and their evaluation order are carried over verbatim from
/// the pre-composition implementation — they are load-bearing for cross-space
/// data isolation.
fn add_shared_space_fanout(
    tx: &Transaction,
    table_name: &str,
    pks: &[String],
) -> Result<(), CrdtSetupError> {
    // The per-space delete-log must NOT carry the crate's generic
    // BEFORE-DELETE trigger: retention pruning
    // (`compaction_anchor::prune_shared_space_delete_log_and_advance_anchors`)
    // hard-deletes from it with `triggers_enabled = '1'`, so the generic
    // trigger would append one `haex_deleted_rows` event per pruned row and
    // ship it on the owner-domain sync — which would then delete the peer's
    // own per-space delete-log entries. Same rationale as the crate's
    // `haex_deleted_rows` exemption; `haex_deleted_rows` is the only table
    // the crate knows about, so vault suppresses its own log's trigger here
    // (see `TRIGGER_VERSION` history v5/v6 in `database::init`).
    if table_name == SHARED_SPACE_DELETED_ROWS_TABLE {
        tx.execute_batch(&drop_trigger_sql(
            &CRATE_DELETE_TRIGGER_TPL.replace("{TABLE_NAME}", table_name),
        ))?;
    }

    // Register-DELETE fanout: additionally emit a per-space delete-log signal
    // (ADR 0002 §6.5). Owner-domain sync continues to receive the standard
    // `haex_deleted_rows` row from the crate's BEFORE-DELETE trigger; the
    // shared-space-domain gets its own signal here.
    //
    // Guard: only install the fanout when the target table exists (migration
    // 0013 creates it). Older test fixtures that hand-build `haex_shared_space_sync`
    // without the new table stay compatible — the fanout is a hard error path
    // otherwise (SQLITE cannot open a trigger whose target doesn't exist).
    let delete_log_present =
        !haex_crdt::get_table_schema(tx, SHARED_SPACE_DELETED_ROWS_TABLE)?.is_empty();

    if table_name == SHARED_SPACE_SYNC_TABLE && delete_log_present {
        let fanout_sql = generate_shared_space_sync_delete_fanout_trigger_sql();
        tx.execute_batch(&fanout_sql)?;
    }

    // Task 5 Path A: direct-emit trigger for the 5 space-scoped infra tables.
    // These carry `space_id` inline and are register-denylisted, so a direct
    // BEFORE-DELETE emit is the only way to reach the per-space delete-log.
    if SPACE_SCOPED_INFRA_TABLES.contains(&table_name) && delete_log_present {
        let sql = generate_shared_space_infra_emit_trigger_sql(table_name, pks);
        tx.execute_batch(&sql)?;
    }

    // Task 5 Path B: register-cascade trigger for every non-exempt table.
    // No-op for infra tables (they are register-denylisted so the WHERE
    // matches zero rows), but keeping the trigger uniform avoids maintaining
    // a second denylist here. The register-DELETE fanout (Task 4) does the
    // per-space emission when this trigger cleans up register entries for
    // extension tables.
    //
    // Guard: the target table (register) must exist. If it doesn't yet, the
    // caller is a legacy fixture — skip and stay compatible.
    let register_present = !haex_crdt::get_table_schema(tx, SHARED_SPACE_SYNC_TABLE)?.is_empty();
    if register_present && !SHARED_SPACE_CASCADE_EXEMPT.contains(&table_name) {
        let sql = generate_shared_space_register_cascade_trigger_sql(table_name, pks);
        tx.execute_batch(&sql)?;
    }

    Ok(())
}

/// Drops the shared-space triggers for `table_name`, mirroring the install
/// gates in [`add_shared_space_fanout`]. Unconditional (`IF EXISTS`) — the
/// `delete_log_present` / `register_present` probes are install-time only, so
/// a drop always leaves a fully clean state.
///
/// Private: it interpolates `table_name` into SQL and relies on the caller
/// having validated it (both call sites go through a `haex_crdt` entry point
/// that rejects unsafe identifiers first).
fn drop_shared_space_triggers(
    tx: &Transaction,
    table_name: &str,
) -> Result<(), CrdtSetupError> {
    let mut sql_batch = String::new();

    // Register-DELETE fanout trigger (Task 4) is scoped to
    // `haex_shared_space_sync`.
    if table_name == SHARED_SPACE_SYNC_TABLE {
        sql_batch.push_str(&drop_trigger_sql(SHARED_SPACE_DELETE_FANOUT_TRIGGER_TPL));
        sql_batch.push('\n');
    }

    // Task 5 infra-emit and register-cascade triggers — parity with setup.
    if SPACE_SCOPED_INFRA_TABLES.contains(&table_name) {
        sql_batch.push_str(&drop_trigger_sql(
            &SHARED_SPACE_INFRA_EMIT_TRIGGER_TPL.replace("{TABLE_NAME}", table_name),
        ));
        sql_batch.push('\n');
    }
    if !SHARED_SPACE_CASCADE_EXEMPT.contains(&table_name) {
        sql_batch.push_str(&drop_trigger_sql(
            &SHARED_SPACE_REGISTER_CASCADE_TRIGGER_TPL.replace("{TABLE_NAME}", table_name),
        ));
        sql_batch.push('\n');
    }

    if !sql_batch.is_empty() {
        tx.execute_batch(&sql_batch)?;
    }
    Ok(())
}

/// Primary-key column names of `table_name`, in schema-declaration order.
/// Empty when the table does not exist.
///
/// The shared-space generators need the PK list; reading it back through
/// [`haex_crdt::get_table_schema`] keeps it out of the crate's installer
/// signature.
fn primary_key_columns(
    conn: &Connection,
    table_name: &str,
) -> Result<Vec<String>, CrdtSetupError> {
    Ok(haex_crdt::get_table_schema(conn, table_name)?
        .into_iter()
        .filter(|c| c.is_pk)
        .map(|c| c.name)
        .collect())
}

/// Generiert das SQL zum Löschen eines Triggers.
fn drop_trigger_sql(trigger_name: &str) -> String {
    format!("DROP TRIGGER IF EXISTS \"{trigger_name}\";")
}

/// Task 5 Path A: Generates SQL for the direct-emit BEFORE-DELETE trigger
/// on a space-scoped infra table.
///
/// Space-scoped infra rows carry `space_id` inline; the trigger reads it from
/// OLD and emits one row into `haex_shared_space_deleted_rows` per DELETE.
/// Register cleanup is not needed (these tables are denylisted from being
/// register targets — see `is_register_target_forbidden`).
fn generate_shared_space_infra_emit_trigger_sql(table_name: &str, pks: &[String]) -> String {
    let trigger_name = SHARED_SPACE_INFRA_EMIT_TRIGGER_TPL.replace("{TABLE_NAME}", table_name);
    let row_pks_json = pks
        .iter()
        .map(|name| format!("'{name}', OLD.\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "CREATE TRIGGER IF NOT EXISTS \"{trigger_name}\"
            BEFORE DELETE ON \"{table_name}\"
            FOR EACH ROW
            WHEN (SELECT COALESCE(value, '1') FROM {TABLE_CRDT_CONFIGS} WHERE key = 'triggers_enabled') = '1'
            BEGIN
            INSERT INTO {SHARED_SPACE_DELETED_ROWS_TABLE}
                (id, space_id, table_name, row_pks, {HLC_TIMESTAMP_COLUMN}, {COLUMN_HLCS_COLUMN})
            VALUES (
                {UUID_FUNCTION_NAME}(),
                OLD.space_id,
                '{table_name}',
                json_object({row_pks_json}),
                {HLC_FUNCTION_NAME}(),
                '{{}}'
            );
            INSERT OR REPLACE INTO {TABLE_CRDT_DIRTY_TABLES} (table_name, last_modified)
            VALUES ('{SHARED_SPACE_DELETED_ROWS_TABLE}', datetime('now'));
            END;"
    )
}

/// Task 5 Path B: Generates SQL for the register-cascade BEFORE-DELETE
/// trigger.
///
/// A row that has ever been shared into a space carries entries in
/// `haex_shared_space_sync` for every owning space. Hard-deleting the row
/// must remove those register entries so the register-DELETE fanout trigger
/// (Task 4) can fan out per-space delete-log signals.
///
/// For space-scoped infra tables this DELETE is a no-op — they are register
/// denylisted, so no matching register rows exist. Path A above handles their
/// direct emit. Path B stays generic to cover both infra and extension.
///
/// **row_pks canonical encoding contract.** The trigger's `json_object(...)`
/// call produces the string `{"pk1":"v1","pk2":"v2",...}` with keys in
/// **primary-key-definition order** (as returned by `PRAGMA table_info`) and
/// no whitespace. Every production writer inserting into
/// `haex_shared_space_sync` MUST use the same encoding, otherwise this
/// BEFORE-DELETE trigger's `WHERE row_pks = json_object(...)` will not match
/// the stored register entry and the cascade will silently no-op.
///
/// Follow-up: audit all Rust register-insert sites (see grep hits for
/// `INSERT INTO haex_shared_space_sync` outside test setups) and factor the
/// encoding into a shared helper to make divergence a compile error rather
/// than a runtime miss.
fn generate_shared_space_register_cascade_trigger_sql(table_name: &str, pks: &[String]) -> String {
    let trigger_name =
        SHARED_SPACE_REGISTER_CASCADE_TRIGGER_TPL.replace("{TABLE_NAME}", table_name);
    let row_pks_json = pks
        .iter()
        .map(|name| format!("'{name}', OLD.\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "CREATE TRIGGER IF NOT EXISTS \"{trigger_name}\"
            BEFORE DELETE ON \"{table_name}\"
            FOR EACH ROW
            WHEN (SELECT COALESCE(value, '1') FROM {TABLE_CRDT_CONFIGS} WHERE key = 'triggers_enabled') = '1'
            BEGIN
            DELETE FROM {SHARED_SPACE_SYNC_TABLE}
            WHERE table_name = '{table_name}'
              AND row_pks = json_object({row_pks_json});
            END;"
    )
}

/// Generates SQL for the register-DELETE fanout trigger.
///
/// Installed in addition to the generic BEFORE-DELETE trigger on
/// `haex_shared_space_sync`. Whenever a register entry is removed (unshare
/// or business-table DELETE cascade — Task 5), this trigger emits a per-space
/// signal into `haex_shared_space_deleted_rows` so every space member's
/// apply-path (Task 6) can converge on the removal.
///
/// The row_pks column carries the business-row identity JSON verbatim from
/// the register — receivers use it to reconstruct the target-row WHERE clause.
///
/// Gated by `triggers_enabled` so the apply-path can DELETE from the register
/// during row-plus-register removal without re-emitting.
fn generate_shared_space_sync_delete_fanout_trigger_sql() -> String {
    format!(
        "CREATE TRIGGER IF NOT EXISTS \"{SHARED_SPACE_DELETE_FANOUT_TRIGGER_TPL}\"
            BEFORE DELETE ON \"{SHARED_SPACE_SYNC_TABLE}\"
            FOR EACH ROW
            WHEN (SELECT COALESCE(value, '1') FROM {TABLE_CRDT_CONFIGS} WHERE key = 'triggers_enabled') = '1'
            BEGIN
            INSERT INTO {SHARED_SPACE_DELETED_ROWS_TABLE}
                (id, space_id, table_name, row_pks, {HLC_TIMESTAMP_COLUMN}, {COLUMN_HLCS_COLUMN})
            VALUES (
                {UUID_FUNCTION_NAME}(),
                OLD.space_id,
                OLD.table_name,
                OLD.row_pks,
                {HLC_FUNCTION_NAME}(),
                '{{}}'
            );
            INSERT OR REPLACE INTO {TABLE_CRDT_DIRTY_TABLES} (table_name, last_modified)
            VALUES ('{SHARED_SPACE_DELETED_ROWS_TABLE}', datetime('now'));
            END;"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Test that ensure_crdt_columns adds the same columns that the CrdtTransformer
    /// would add to a CREATE TABLE statement.
    /// This ensures consistency between the two approaches.
    #[test]
    fn test_ensure_crdt_columns_consistency_with_transformer() {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute(
            "CREATE TABLE test_table (id TEXT PRIMARY KEY, name TEXT)",
            [],
        )
        .unwrap();

        let tx = conn.unchecked_transaction().unwrap();

        let result = ensure_crdt_columns(&tx, "test_table").unwrap();
        assert!(result, "Should have added columns");

        tx.commit().unwrap();

        let columns = get_table_schema(&conn, "test_table").unwrap();
        let column_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();

        assert!(
            column_names.contains(&HLC_TIMESTAMP_COLUMN),
            "Missing {} column. Found: {:?}",
            HLC_TIMESTAMP_COLUMN,
            column_names
        );
        assert!(
            column_names.contains(&COLUMN_HLCS_COLUMN),
            "Missing {} column. Found: {:?}",
            COLUMN_HLCS_COLUMN,
            column_names
        );
        assert!(
            column_names.contains(&COLUMN_SIGS_COLUMN),
            "Missing {} column. Found: {:?}",
            COLUMN_SIGS_COLUMN,
            column_names
        );
    }

    #[test]
    fn test_ensure_crdt_columns_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute(
            &format!(
                "CREATE TABLE test_table (
                    id TEXT PRIMARY KEY,
                    name TEXT,
                    {} TEXT,
                    {} TEXT NOT NULL DEFAULT '{{}}',
                    {} TEXT NOT NULL DEFAULT '{{}}'
                )",
                HLC_TIMESTAMP_COLUMN, COLUMN_HLCS_COLUMN, COLUMN_SIGS_COLUMN
            ),
            [],
        )
        .unwrap();

        let tx = conn.unchecked_transaction().unwrap();

        let result = ensure_crdt_columns(&tx, "test_table").unwrap();
        assert!(!result, "Should not have added any columns");

        tx.commit().unwrap();
    }

    #[test]
    fn test_ensure_crdt_columns_partial() {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute(
            &format!(
                "CREATE TABLE test_table (
                    id TEXT PRIMARY KEY,
                    name TEXT,
                    {} TEXT
                )",
                HLC_TIMESTAMP_COLUMN
            ),
            [],
        )
        .unwrap();

        let tx = conn.unchecked_transaction().unwrap();

        let result = ensure_crdt_columns(&tx, "test_table").unwrap();
        assert!(result, "Should have added missing columns");

        tx.commit().unwrap();

        let columns = get_table_schema(&conn, "test_table").unwrap();
        let column_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();

        assert!(column_names.contains(&HLC_TIMESTAMP_COLUMN));
        assert!(column_names.contains(&COLUMN_HLCS_COLUMN));
        assert!(column_names.contains(&COLUMN_SIGS_COLUMN));
    }

    // =====================================================================
    // Task 4 — Register-DELETE fanout trigger.
    //
    // Deleting a row from `haex_shared_space_sync` (register) must produce a
    // per-space signal in `haex_shared_space_deleted_rows` so other members
    // of the space converge on the removal (unshare or hard-delete). Owner-
    // domain sync continues to receive its signal via the standard
    // `haex_deleted_rows` trigger installed by the crate's installer.
    //
    // Aus ADR 0002 §6.5 (revised 2026-07-29).
    // =====================================================================

    use rusqlite::functions::FunctionFlags;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn register_shared_space_test_udfs(conn: &Connection) {
        // Fresh test UDFs: gen_uuid returns unique strings, current_hlc returns
        // a monotonically increasing string so LWW ordering is well-defined.
        static UUID_COUNTER: AtomicU64 = AtomicU64::new(0);
        static HLC_COUNTER: AtomicU64 = AtomicU64::new(0);

        conn.create_scalar_function(
            UUID_FUNCTION_NAME,
            0,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
            |_| {
                Ok(format!(
                    "test-uuid-{}",
                    UUID_COUNTER.fetch_add(1, Ordering::Relaxed)
                ))
            },
        )
        .expect("register gen_uuid");
        conn.create_scalar_function(
            HLC_FUNCTION_NAME,
            0,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
            |_| {
                Ok(format!(
                    "hlc-{:016}",
                    HLC_COUNTER.fetch_add(1, Ordering::Relaxed)
                ))
            },
        )
        .expect("register current_hlc");
    }

    /// Builds a bare in-memory DB with the minimal CRDT plumbing needed to
    /// exercise the register-DELETE fanout trigger. Doesn't touch the real
    /// migration file — the schemas here match production so a divergence in
    /// column names shows up immediately.
    fn setup_register_delete_fixture() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        register_shared_space_test_udfs(&conn);

        // Bookkeeping tables the triggers read/write.
        conn.execute_batch(
            "CREATE TABLE haex_crdt_configs_no_sync (
                 key TEXT PRIMARY KEY NOT NULL,
                 value TEXT,
                 type TEXT
             );
             CREATE TABLE haex_crdt_dirty_tables_no_sync (
                 table_name TEXT PRIMARY KEY NOT NULL,
                 last_modified TEXT
             );
             INSERT INTO haex_crdt_configs_no_sync (key, value, type)
             VALUES ('triggers_enabled', '1', 'boolean');",
        )
        .unwrap();

        // Owner-domain delete-log (same shape as production 0000 migration
        // plus CRDT meta cols the transformer injects at CREATE-TABLE time).
        conn.execute_batch(
            "CREATE TABLE haex_deleted_rows (
                 id TEXT PRIMARY KEY NOT NULL,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
                 haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
             );",
        )
        .unwrap();

        // Register table.
        conn.execute_batch(
            "CREATE TABLE haex_shared_space_sync (
                 id TEXT PRIMARY KEY NOT NULL,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 space_id TEXT NOT NULL,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
                 haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
             );",
        )
        .unwrap();

        // Shared-space-domain delete-log (Migration 0013 + CRDT meta cols).
        conn.execute_batch(
            "CREATE TABLE haex_shared_space_deleted_rows (
                 id TEXT PRIMARY KEY NOT NULL,
                 space_id TEXT NOT NULL,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
                 haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
             );",
        )
        .unwrap();

        let tx = conn.unchecked_transaction().unwrap();
        install_crdt_with_shared_space(&tx, "haex_shared_space_sync", false)
            .expect("register triggers");
        tx.commit().unwrap();

        conn
    }

    fn trigger_exists(conn: &Connection, trigger_name: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'trigger' AND name = ?",
            [trigger_name],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn deleting_register_entry_writes_shared_space_delete_log_row() {
        let conn = setup_register_delete_fixture();

        // Seed a register row saying "table T row {id:R} shared into SPACE_X".
        conn.execute(
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc_no_sync)
             VALUES ('reg-1', 'haex_peer_shares', '{\"id\":\"R\"}', 'SPACE_X', 'hlc-seed')",
            [],
        )
        .unwrap();

        // Delete the register row (models an unshare or the cascade from a
        // business-table DELETE — see Task 5).
        conn.execute("DELETE FROM haex_shared_space_sync WHERE id = 'reg-1'", [])
            .unwrap();

        // Assert: a delete-log row landed with the correct per-space info.
        let rows: Vec<(String, String, String)> = conn
            .prepare("SELECT space_id, table_name, row_pks FROM haex_shared_space_deleted_rows")
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(
            rows.len(),
            1,
            "exactly one shared-space-delete-log row expected, got {rows:?}"
        );
        assert_eq!(
            rows[0],
            (
                "SPACE_X".to_string(),
                "haex_peer_shares".to_string(),
                r#"{"id":"R"}"#.to_string()
            )
        );
    }

    #[test]
    fn deleting_register_entry_gated_by_triggers_enabled_flag() {
        // When triggers_enabled=0 the fanout must NOT fire — this is the
        // apply-path gate that lets the receiver clear the register without
        // re-emitting a delete-log entry (which would loop).
        let conn = setup_register_delete_fixture();
        conn.execute(
            "UPDATE haex_crdt_configs_no_sync SET value = '0' WHERE key = 'triggers_enabled'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc_no_sync)
             VALUES ('reg-1', 'haex_peer_shares', '{\"id\":\"R\"}', 'SPACE_X', 'hlc-seed')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM haex_shared_space_sync WHERE id = 'reg-1'", [])
            .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM haex_shared_space_deleted_rows",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "delete-log must not receive a row when triggers_enabled=0"
        );
    }

    // =====================================================================
    // Task 5 — Business-table DELETE cascade.
    //
    // Two mechanisms, one goal (per-space delete propagation, ADR 0002 §6.5):
    //
    // A. Space-scoped infra tables (haex_space_members, haex_peer_shares,
    //    haex_space_devices, haex_mls_sync_keys, haex_device_mls_enrollments)
    //    carry space_id NOT NULL and are denylisted from the register. A
    //    direct BEFORE-DELETE trigger emits into
    //    haex_shared_space_deleted_rows using OLD.space_id.
    //
    // B. Extension tables (anything else that isn't infra-of-infra) may live
    //    in multiple spaces via the register. A BEFORE-DELETE trigger cleans
    //    the register entries; the register-DELETE fanout from Task 4 then
    //    produces per-space signals.
    // =====================================================================

    fn setup_business_delete_fixture() -> Connection {
        // Reuse the register-DELETE fixture (has all bookkeeping tables +
        // register triggers) and layer business tables on top.
        let conn = setup_register_delete_fixture();

        // A representative space-scoped infra table (path A) — schema mirrors
        // haex_peer_shares' relevant columns.
        conn.execute_batch(
            "CREATE TABLE haex_peer_shares (
                 id TEXT PRIMARY KEY NOT NULL,
                 space_id TEXT NOT NULL,
                 name TEXT NOT NULL,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
                 haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
             );",
        )
        .unwrap();

        // A representative extension table (path B) — arbitrary schema, no
        // space_id column; ownership lives in the register.
        conn.execute_batch(
            "CREATE TABLE ext_notes_items (
                 id TEXT PRIMARY KEY NOT NULL,
                 body TEXT,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
                 haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
             );",
        )
        .unwrap();

        let tx = conn.unchecked_transaction().unwrap();
        install_crdt_with_shared_space(&tx, "haex_peer_shares", false).expect("infra triggers");
        install_crdt_with_shared_space(&tx, "ext_notes_items", false).expect("ext triggers");
        tx.commit().unwrap();

        conn
    }

    fn delete_log_rows(conn: &Connection) -> Vec<(String, String, String)> {
        conn.prepare(
            "SELECT space_id, table_name, row_pks \
             FROM haex_shared_space_deleted_rows \
             ORDER BY space_id, table_name",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    #[test]
    fn hard_delete_of_space_scoped_infra_row_emits_one_per_space_delete_log_entry() {
        // Path A: haex_peer_shares row lives in exactly one space via
        // OLD.space_id; DELETE emits exactly one delete-log signal.
        let conn = setup_business_delete_fixture();
        conn.execute(
            "INSERT INTO haex_peer_shares (id, space_id, name, haex_hlc_no_sync)
             VALUES ('share-1', 'SPACE_X', 'Folder', 'hlc-seed')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM haex_peer_shares WHERE id = 'share-1'", [])
            .unwrap();

        let rows = delete_log_rows(&conn);
        assert_eq!(
            rows,
            vec![(
                "SPACE_X".to_string(),
                "haex_peer_shares".to_string(),
                r#"{"id":"share-1"}"#.to_string(),
            )],
            "hard delete of an infra row must emit exactly one per-space signal"
        );
    }

    #[test]
    fn hard_delete_of_extension_row_shared_into_many_spaces_emits_one_per_space() {
        // Path B: an extension row lives in multiple spaces via register
        // entries. Hard-deleting the row must cascade to register cleanup;
        // the register-DELETE fanout (Task 4) then emits per-space signals.
        let conn = setup_business_delete_fixture();
        conn.execute(
            "INSERT INTO ext_notes_items (id, body, haex_hlc_no_sync)
             VALUES ('note-1', 'hello', 'hlc-seed')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc_no_sync)
             VALUES ('reg-x', 'ext_notes_items', '{\"id\":\"note-1\"}', 'SPACE_X', 'hlc-1'),
                    ('reg-y', 'ext_notes_items', '{\"id\":\"note-1\"}', 'SPACE_Y', 'hlc-2'),
                    ('reg-z', 'ext_notes_items', '{\"id\":\"note-1\"}', 'SPACE_Z', 'hlc-3')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM ext_notes_items WHERE id = 'note-1'", [])
            .unwrap();

        let rows = delete_log_rows(&conn);
        assert_eq!(
            rows.len(),
            3,
            "one signal per registered space, got {rows:?}"
        );
        let space_ids: Vec<&str> = rows.iter().map(|(s, _, _)| s.as_str()).collect();
        assert_eq!(space_ids, vec!["SPACE_X", "SPACE_Y", "SPACE_Z"]);
        for (_, table, pks) in &rows {
            assert_eq!(table, "ext_notes_items");
            assert_eq!(pks, r#"{"id":"note-1"}"#);
        }
        // Register itself is now empty for this row.
        let register_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM haex_shared_space_sync \
                 WHERE table_name = 'ext_notes_items' AND row_pks = '{\"id\":\"note-1\"}'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            register_count, 0,
            "register entries must be gone after cascade"
        );
    }

    #[test]
    fn hard_delete_of_infra_row_does_not_double_emit_from_register_cascade() {
        // Regression guard: infra tables are denylisted from the register.
        // The cascade DELETE FROM register must therefore find zero matching
        // register rows — the ONLY signal must come from Path A's direct
        // emit. If both fired we'd see two rows for the same delete.
        let conn = setup_business_delete_fixture();
        conn.execute(
            "INSERT INTO haex_peer_shares (id, space_id, name, haex_hlc_no_sync)
             VALUES ('share-1', 'SPACE_X', 'Folder', 'hlc-seed')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM haex_peer_shares WHERE id = 'share-1'", [])
            .unwrap();

        let rows = delete_log_rows(&conn);
        assert_eq!(
            rows.len(),
            1,
            "exactly one signal for an infra delete, no double-emit; got {rows:?}"
        );
    }

    #[test]
    fn test_ensure_crdt_columns_nonexistent_table() {
        let conn = Connection::open_in_memory().unwrap();
        let tx = conn.unchecked_transaction().unwrap();

        // Should return false for non-existent table
        let result = ensure_crdt_columns(&tx, "nonexistent_table").unwrap();
        assert!(!result, "Should return false for non-existent table");
    }

    // =====================================================================
    // Composition guards: the crate installs a generic BEFORE-DELETE trigger
    // on every table but `haex_deleted_rows`. Vault's own per-space
    // delete-log must not carry one either (TRIGGER_VERSION v6), so the
    // shared-space layer suppresses it.
    // =====================================================================

    #[test]
    fn install_on_shared_space_delete_log_suppresses_generic_delete_trigger() {
        let conn = setup_register_delete_fixture();

        let tx = conn.unchecked_transaction().unwrap();
        install_crdt_with_shared_space(&tx, SHARED_SPACE_DELETED_ROWS_TABLE, false)
            .expect("delete-log triggers");
        tx.commit().unwrap();

        assert!(
            !trigger_exists(&conn, "z_dirty_haex_shared_space_deleted_rows_delete"),
            "the per-space delete-log must not carry the crate's generic \
             BEFORE-DELETE trigger — retention pruning would re-log every \
             pruned row into haex_deleted_rows"
        );
        // The INSERT/UPDATE triggers the crate installs are wanted.
        assert!(trigger_exists(
            &conn,
            "z_dirty_haex_shared_space_deleted_rows_insert"
        ));
        assert!(trigger_exists(
            &conn,
            "z_dirty_haex_shared_space_deleted_rows_update"
        ));
    }

    #[test]
    fn pruning_the_shared_space_delete_log_does_not_append_to_owner_delete_log() {
        // Behavioural half of the guard above: this models
        // `compaction_anchor::prune_shared_space_delete_log_and_advance_anchors`,
        // which hard-deletes with triggers_enabled = '1'.
        let conn = setup_register_delete_fixture();

        let tx = conn.unchecked_transaction().unwrap();
        install_crdt_with_shared_space(&tx, SHARED_SPACE_DELETED_ROWS_TABLE, false)
            .expect("delete-log triggers");
        tx.commit().unwrap();

        conn.execute(
            "INSERT INTO haex_shared_space_deleted_rows \
             (id, space_id, table_name, row_pks, haex_hlc_no_sync) \
             VALUES ('sig-1', 'SPACE_X', 'ext_notes_items', '{\"id\":\"note-1\"}', 'hlc-1')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM haex_shared_space_deleted_rows", [])
            .unwrap();

        let owner_log_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM haex_deleted_rows", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            owner_log_rows, 0,
            "retention pruning of the per-space delete-log must not emit \
             owner-domain delete events"
        );
    }

    #[test]
    fn drop_removes_both_the_generic_and_the_shared_space_triggers() {
        // Parity guard for the drop wrapper: it must clear the crate's three
        // and vault's shared-space triggers in one call.
        let conn = setup_business_delete_fixture();

        assert!(trigger_exists(&conn, "z_shared_space_delete_fanout"));
        assert!(trigger_exists(
            &conn,
            "z_shared_space_infra_emit_haex_peer_shares_delete"
        ));
        assert!(trigger_exists(
            &conn,
            "z_shared_space_register_cascade_ext_notes_items_delete"
        ));

        let tx = conn.unchecked_transaction().unwrap();
        drop_crdt_with_shared_space(&tx, "haex_shared_space_sync").unwrap();
        drop_crdt_with_shared_space(&tx, "haex_peer_shares").unwrap();
        drop_crdt_with_shared_space(&tx, "ext_notes_items").unwrap();
        tx.commit().unwrap();

        for name in [
            "z_shared_space_delete_fanout",
            "z_shared_space_infra_emit_haex_peer_shares_delete",
            "z_shared_space_register_cascade_ext_notes_items_delete",
            "z_dirty_haex_shared_space_sync_insert",
            "z_dirty_haex_peer_shares_update",
            "z_dirty_ext_notes_items_delete",
        ] {
            assert!(!trigger_exists(&conn, name), "{name} should be gone");
        }
    }

    #[test]
    fn ensure_layers_the_register_cascade_onto_a_table_that_already_has_generic_triggers() {
        // The pre-composition `ensure_crdt_columns_and_triggers` short-
        // circuited on a single `z_dirty_{table}_insert` probe, so a table
        // that already had the generic triggers could never acquire its
        // register-cascade trigger. The composed wrapper applies the
        // shared-space layer unconditionally.
        let conn = setup_register_delete_fixture();
        conn.execute_batch(
            "CREATE TABLE ext_notes_items (
                 id TEXT PRIMARY KEY NOT NULL,
                 body TEXT,
                 haex_hlc_no_sync TEXT,
                 haex_column_hlcs_no_sync TEXT NOT NULL DEFAULT '{}',
                 haex_column_sigs_no_sync TEXT NOT NULL DEFAULT '{}'
             );",
        )
        .unwrap();

        // Install only the crate's generic layer, so the cascade trigger is
        // absent while all three generic triggers are present.
        let tx = conn.unchecked_transaction().unwrap();
        haex_crdt::setup_triggers_for_table(&tx, "ext_notes_items", false).unwrap();
        tx.commit().unwrap();
        assert!(!trigger_exists(
            &conn,
            "z_shared_space_register_cascade_ext_notes_items_delete"
        ));

        let tx = conn.unchecked_transaction().unwrap();
        let (columns_added, _) = ensure_crdt_columns_and_triggers(&tx, "ext_notes_items").unwrap();
        tx.commit().unwrap();

        assert!(!columns_added, "the fixture table already has all three");
        assert!(
            trigger_exists(&conn, "z_shared_space_register_cascade_ext_notes_items_delete"),
            "ensure must layer the shared-space cascade on regardless of the \
             generic triggers already being present"
        );
    }

    #[test]
    fn ensure_on_nonexistent_table_does_not_install_shared_space_triggers() {
        let conn = setup_register_delete_fixture();

        let tx = conn.unchecked_transaction().unwrap();
        let (columns_added, triggers_created) =
            ensure_crdt_columns_and_triggers(&tx, "table_that_does_not_exist").unwrap();
        tx.commit().unwrap();

        assert!(!columns_added);
        assert!(!triggers_created);
        assert!(!trigger_exists(
            &conn,
            "z_shared_space_register_cascade_table_that_does_not_exist_delete"
        ));
    }
}

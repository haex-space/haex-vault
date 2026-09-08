// src-tauri/src/crdt/shared_space_trigger/mod.rs
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
// app reach the crate's generic surface through this module's path. A later
// batch flattens them to `haex_crdt` imports.

use rusqlite::{Connection, Transaction};

mod generators;

use generators::{
    generate_shared_space_infra_emit_trigger_sql,
    generate_shared_space_register_cascade_trigger_sql,
    generate_shared_space_sync_delete_fanout_trigger_sql,
};

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
fn drop_shared_space_triggers(tx: &Transaction, table_name: &str) -> Result<(), CrdtSetupError> {
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
fn primary_key_columns(conn: &Connection, table_name: &str) -> Result<Vec<String>, CrdtSetupError> {
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

#[cfg(test)]
mod tests;

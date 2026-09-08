//! SQL generators for vault's shared-space triggers.
//!
//! Split out of `mod.rs` so the composition wrappers and the DDL text they
//! emit stay separately readable. All three are private to the module: the
//! only way to install them is through
//! [`super::install_crdt_with_shared_space`].

use super::{SHARED_SPACE_DELETED_ROWS_TABLE, SHARED_SPACE_SYNC_TABLE};
use super::{
    SHARED_SPACE_DELETE_FANOUT_TRIGGER_TPL, SHARED_SPACE_INFRA_EMIT_TRIGGER_TPL,
    SHARED_SPACE_REGISTER_CASCADE_TRIGGER_TPL,
};
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};
use haex_crdt::crdt::columns::{
    COLUMN_HLCS_COLUMN, HLC_FUNCTION_NAME, HLC_TIMESTAMP_COLUMN, UUID_FUNCTION_NAME,
};

/// Task 5 Path A: Generates SQL for the direct-emit BEFORE-DELETE trigger
/// on a space-scoped infra table.
///
/// Space-scoped infra rows carry `space_id` inline; the trigger reads it from
/// OLD and emits one row into `haex_shared_space_deleted_rows` per DELETE.
/// Register cleanup is not needed (these tables are denylisted from being
/// register targets — see `is_register_target_forbidden`).
pub(super) fn generate_shared_space_infra_emit_trigger_sql(
    table_name: &str,
    pks: &[String],
) -> String {
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
pub(super) fn generate_shared_space_register_cascade_trigger_sql(
    table_name: &str,
    pks: &[String],
) -> String {
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
pub(super) fn generate_shared_space_sync_delete_fanout_trigger_sql() -> String {
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

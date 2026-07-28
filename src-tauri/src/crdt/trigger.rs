// src-tauri/src/crdt/trigger.rs
//
// New approach: Instead of logging changes to haex_crdt_changes table,
// we just mark tables as "dirty" in haex_crdt_dirty_tables.
// Actual sync happens by scanning the dirty tables directly.
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES};
use rusqlite::{Connection, Result as RusqliteResult, Row, Transaction};
use serde::Serialize;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use ts_rs::TS;

// Trigger names for dirty table tracking
const INSERT_TRIGGER_TPL: &str = "z_dirty_{TABLE_NAME}_insert";
const UPDATE_TRIGGER_TPL: &str = "z_dirty_{TABLE_NAME}_update";
const DELETE_TRIGGER_TPL: &str = "z_dirty_{TABLE_NAME}_delete";

pub const HLC_TIMESTAMP_COLUMN: &str = "haex_hlc";
pub const COLUMN_HLCS_COLUMN: &str = "haex_column_hlcs";
/// Per-column author signatures (JSON `{ column_name -> base64 sig }`).
///
/// Parallel to `haex_column_hlcs`: while `haex_column_hlcs` tracks the last
/// HLC per column for LWW, `haex_column_sigs` tracks the signature over the
/// authoritative preimage for the last write to that column. Shared-space
/// receivers verify against this map before applying an incoming column
/// change (Phase 1 of the shared-space authenticity design).
pub const COLUMN_SIGS_COLUMN: &str = "haex_column_sigs";

/// Name der Delete-Log-Tabelle (Sync-Tabelle, daher ohne `_no_sync`-Suffix).
/// Deletes werden hier als Event-Zeilen festgehalten; die Haupttabellen enthalten
/// keine Tombstone-Spalten mehr.
pub const DELETED_ROWS_TABLE: &str = "haex_deleted_rows";

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

// Sync metadata columns that should NOT be tracked (to prevent trigger loops)
const LAST_PUSH_HLC_COLUMN: &str = "last_push_hlc_timestamp";
const LAST_PULL_SERVER_TIMESTAMP_COLUMN: &str = "last_pull_server_timestamp";
const UPDATED_AT_COLUMN: &str = "updated_at";
const CREATED_AT_COLUMN: &str = "created_at";

/// Name der custom UUID-Generierungs-Funktion (registriert in database::core::open_and_init_db)
pub const UUID_FUNCTION_NAME: &str = "gen_uuid";

/// Name der transaction-scoped HLC UDF (registriert in database::core::open_and_init_db).
/// Gibt denselben Timestamp für alle Aufrufe innerhalb einer Transaktion zurück.
pub const HLC_FUNCTION_NAME: &str = "current_hlc";

#[derive(Debug)]
pub enum CrdtSetupError {
    /// Kapselt einen Fehler, der von der rusqlite-Bibliothek kommt.
    DatabaseError(rusqlite::Error),
    HlcColumnMissing {
        table_name: String,
        column_name: String,
    },
    /// Die Tabelle hat keinen Primärschlüssel, was eine CRDT-Voraussetzung ist.
    PrimaryKeyMissing { table_name: String },
}

// Implementierung, damit unser Error-Typ schön formatiert werden kann.
impl Display for CrdtSetupError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CrdtSetupError::DatabaseError(e) => write!(f, "Database error: {e}"),
            CrdtSetupError::HlcColumnMissing {
                table_name,
                column_name,
            } => write!(
                f,
                "Table '{table_name}' is missing the required hlc column '{column_name}'"
            ),
            CrdtSetupError::PrimaryKeyMissing { table_name } => {
                write!(f, "Table '{table_name}' has no primary key")
            }
        }
    }
}

// Implementierung, damit unser Typ als "echter" Error erkannt wird.
impl Error for CrdtSetupError {}

// Wichtige Konvertierung: Erlaubt uns, den `?`-Operator auf Funktionen zu verwenden,
// die `rusqlite::Error` zurückgeben. Der Fehler wird automatisch in unseren
// `CrdtSetupError::DatabaseError` verpackt.
impl From<rusqlite::Error> for CrdtSetupError {
    fn from(err: rusqlite::Error) -> Self {
        CrdtSetupError::DatabaseError(err)
    }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub enum TriggerSetupResult {
    Success,
    TableNotFound,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub column_type: String,
    pub is_pk: bool,
}

impl ColumnInfo {
    pub fn from_row(row: &Row) -> RusqliteResult<Self> {
        Ok(ColumnInfo {
            name: row.get("name")?,
            column_type: row.get("type")?,
            is_pk: row.get::<_, i64>("pk")? > 0,
        })
    }
}

pub fn is_safe_identifier(name: &str) -> bool {
    // Allow alphanumeric characters, underscores, and hyphens (for extension names like "nuxt-app")
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// Richtet CRDT-Trigger für eine einzelne Tabelle ein.
pub fn setup_triggers_for_table(
    tx: &Transaction,
    table_name: &str,
    recreate: bool,
) -> Result<TriggerSetupResult, CrdtSetupError> {
    let columns = get_table_schema(tx, table_name)?;

    if columns.is_empty() {
        return Ok(TriggerSetupResult::TableNotFound);
    }

    if !columns.iter().any(|c| c.name == HLC_TIMESTAMP_COLUMN) {
        return Err(CrdtSetupError::HlcColumnMissing {
            table_name: table_name.to_string(),
            column_name: HLC_TIMESTAMP_COLUMN.to_string(),
        });
    }

    let pks: Vec<String> = columns
        .iter()
        .filter(|c| c.is_pk)
        .map(|c| c.name.clone())
        .collect();

    if pks.is_empty() {
        return Err(CrdtSetupError::PrimaryKeyMissing {
            table_name: table_name.to_string(),
        });
    }

    // Calculate columns to track: all columns EXCEPT:
    // - PKs
    // - CRDT columns (haex_hlc, haex_column_hlcs, haex_column_sigs)
    // - Sync metadata columns (to prevent trigger loops)
    let cols_to_track: Vec<String> = columns
        .iter()
        .filter(|c| {
            !c.is_pk
                && c.name != HLC_TIMESTAMP_COLUMN
                && c.name != COLUMN_HLCS_COLUMN
                && c.name != COLUMN_SIGS_COLUMN
                && c.name != LAST_PUSH_HLC_COLUMN
                && c.name != LAST_PULL_SERVER_TIMESTAMP_COLUMN
                && c.name != UPDATED_AT_COLUMN
                && c.name != CREATED_AT_COLUMN
        })
        .map(|c| c.name.clone())
        .collect();

    let insert_trigger_sql = generate_insert_trigger_sql(table_name, &cols_to_track, &pks);
    let update_trigger_sql = generate_update_trigger_sql(table_name, &cols_to_track, &pks);

    if recreate {
        drop_triggers_for_table(tx, table_name)?;
    }

    tx.execute_batch(&insert_trigger_sql)?;
    tx.execute_batch(&update_trigger_sql)?;

    // Der BEFORE-DELETE-Trigger loggt gelöschte Rows nach haex_deleted_rows.
    // Auf der Log-Tabelle selbst würde das Cleanup-DELETEs rekursiv ins Log
    // zurückschreiben — also legen wir für sie keinen DELETE-Trigger an.
    // Sie ist die einzige Tabelle mit dieser Ausnahme.
    if table_name != DELETED_ROWS_TABLE && table_name != SHARED_SPACE_DELETED_ROWS_TABLE {
        let delete_trigger_sql = generate_delete_trigger_sql(table_name, &pks);
        tx.execute_batch(&delete_trigger_sql)?;
    }

    // Register-DELETE fanout: additionally emit a per-space delete-log signal
    // (ADR 0002 §6.5). Owner-domain sync continues to receive the standard
    // `haex_deleted_rows` row from `generate_delete_trigger_sql` above; the
    // shared-space-domain gets its own signal here.
    if table_name == SHARED_SPACE_SYNC_TABLE {
        let fanout_sql = generate_shared_space_sync_delete_fanout_trigger_sql();
        tx.execute_batch(&fanout_sql)?;
    }

    Ok(TriggerSetupResult::Success)
}

/// Holt das Schema für eine gegebene Tabelle.
pub fn get_table_schema(conn: &Connection, table_name: &str) -> RusqliteResult<Vec<ColumnInfo>> {
    if !is_safe_identifier(table_name) {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "Invalid or unsafe table name provided: {table_name}"
        )));
    }

    let sql = format!("PRAGMA table_info(\"{table_name}\");");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], ColumnInfo::from_row)?;
    rows.collect()
}

// get_foreign_key_columns() removed - not needed with hard deletes (no ON CONFLICT logic)

pub fn drop_triggers_for_table(
    tx: &Transaction, // Arbeitet direkt auf einer Transaktion
    table_name: &str,
) -> Result<(), CrdtSetupError> {
    if !is_safe_identifier(table_name) {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "Invalid or unsafe table name provided: {table_name}"
        ))
        .into());
    }

    let drop_insert_trigger_sql =
        drop_trigger_sql(INSERT_TRIGGER_TPL.replace("{TABLE_NAME}", table_name));
    let drop_update_trigger_sql =
        drop_trigger_sql(UPDATE_TRIGGER_TPL.replace("{TABLE_NAME}", table_name));
    let drop_delete_trigger_sql =
        drop_trigger_sql(DELETE_TRIGGER_TPL.replace("{TABLE_NAME}", table_name));

    let mut sql_batch =
        format!("{drop_insert_trigger_sql}\n{drop_update_trigger_sql}\n{drop_delete_trigger_sql}");

    // Register-DELETE fanout trigger (Task 4) is scoped to
    // `haex_shared_space_sync`; drop it alongside the standard triggers so a
    // recreate leaves the DB in a fully clean state.
    if table_name == SHARED_SPACE_SYNC_TABLE {
        sql_batch.push('\n');
        sql_batch.push_str(&drop_trigger_sql(
            SHARED_SPACE_DELETE_FANOUT_TRIGGER_TPL.to_string(),
        ));
    }

    tx.execute_batch(&sql_batch)?;
    Ok(())
}

/* pub fn recreate_triggers_for_table(
    conn: &mut Connection,
    table_name: &str,
) -> Result<TriggerSetupResult, CrdtSetupError> {
    // Starte eine einzige Transaktion für beide Operationen
    let tx = conn.transaction()?;

    // 1. Rufe die Drop-Funktion auf
    drop_triggers_for_table(&tx, table_name)?;

    // 2. Erstelle die Trigger neu (vereinfachte Logik ohne Drop)
    // Wir rufen die `setup_triggers_for_table` Logik hier manuell nach,
    // um die Transaktion weiterzuverwenden.
    let columns = get_table_schema(&tx, table_name)?;

    if columns.is_empty() {
        tx.commit()?; // Wichtig: Transaktion beenden
        return Ok(TriggerSetupResult::TableNotFound);
    }
    // ... (Validierungslogik wiederholen) ...
    if !columns.iter().any(|c| c.name == TOMBSTONE_COLUMN) {
        /* ... */
        return Err(CrdtSetupError::TombstoneColumnMissing {
            table_name: table_name.to_string(),
            column_name: TOMBSTONE_COLUMN.to_string(),
        });
    }
    let pks: Vec<String> = columns
        .iter()
        .filter(|c| c.is_pk)
        .map(|c| c.name.clone())
        .collect();
    if pks.is_empty() {
        /* ... */
        return Err(CrdtSetupError::PrimaryKeyMissing {
            table_name: table_name.to_string(),
        });
    }
    let cols_to_track: Vec<String> = columns
        .iter()
        .filter(|c| !c.is_pk && c.name != TOMBSTONE_COLUMN && c.name != HLC_TIMESTAMP_COLUMN)
        .map(|c| c.name.clone())
        .collect();

    let insert_trigger_sql = generate_insert_trigger_sql(table_name, &pks, &cols_to_track);
    let update_trigger_sql = generate_update_trigger_sql(table_name, &pks, &cols_to_track);
    let sql_batch = format!("{}\n{}", insert_trigger_sql, update_trigger_sql);
    tx.execute_batch(&sql_batch)?;

    // Beende die Transaktion
    tx.commit()?;

    Ok(TriggerSetupResult::Success)
}
 */
/// Generates SQL for INSERT trigger - populates column HLCs and marks table as dirty
fn generate_insert_trigger_sql(
    table_name: &str,
    cols_to_track: &[String],
    primary_key_columns: &[String],
) -> String {
    let trigger_name = INSERT_TRIGGER_TPL.replace("{TABLE_NAME}", table_name);

    // Generate JSON object for haex_column_hlcs with all tracked columns
    let json_pairs: Vec<String> = cols_to_track
        .iter()
        .map(|col| format!("'{}', NEW.\"{}\"", col, HLC_TIMESTAMP_COLUMN))
        .collect();
    let json_object = if json_pairs.is_empty() {
        "'{}'".to_string()
    } else {
        format!("json_object({})", json_pairs.join(", "))
    };

    // Use PK-based WHERE clause to support WITHOUT ROWID tables
    let pk_where = if primary_key_columns.is_empty() {
        "rowid = NEW.rowid".to_string()
    } else {
        primary_key_columns
            .iter()
            .map(|pk| format!("\"{}\" = NEW.\"{}\"", pk, pk))
            .collect::<Vec<_>>()
            .join(" AND ")
    };

    format!(
        "CREATE TRIGGER IF NOT EXISTS \"{trigger_name}\"
            AFTER INSERT ON \"{table_name}\"
            FOR EACH ROW
            WHEN NEW.{HLC_TIMESTAMP_COLUMN} IS NOT NULL
                AND (SELECT COALESCE(value, '1') FROM {TABLE_CRDT_CONFIGS} WHERE key = 'triggers_enabled') = '1'
            BEGIN
            UPDATE \"{table_name}\"
            SET haex_column_hlcs = {json_object}
            WHERE {pk_where};

            INSERT OR REPLACE INTO {TABLE_CRDT_DIRTY_TABLES} (table_name, last_modified)
            VALUES ('{table_name}', datetime('now'));
            END;"
    )
}

/// Generiert das SQL zum Löschen eines Triggers.
fn drop_trigger_sql(trigger_name: String) -> String {
    format!("DROP TRIGGER IF EXISTS \"{trigger_name}\";")
}

/// Generates SQL for UPDATE trigger - updates column HLCs and marks table as dirty
/// IMPORTANT: Only marks table as dirty if at least one TRACKED column changed.
/// This prevents sync loops when only metadata columns (like last_push_hlc_timestamp) are updated.
fn generate_update_trigger_sql(
    table_name: &str,
    cols_to_track: &[String],
    primary_key_columns: &[String],
) -> String {
    let trigger_name = UPDATE_TRIGGER_TPL.replace("{TABLE_NAME}", table_name);

    // Use PK-based WHERE clause to support WITHOUT ROWID tables
    let pk_where = if primary_key_columns.is_empty() {
        "rowid = NEW.rowid".to_string()
    } else {
        primary_key_columns
            .iter()
            .map(|pk| format!("\"{}\" = NEW.\"{}\"", pk, pk))
            .collect::<Vec<_>>()
            .join(" AND ")
    };

    // Generate UPDATE statements for each changed column
    // We check each column individually and update its HLC timestamp if it changed
    let mut update_statements: Vec<String> = Vec::new();

    for col in cols_to_track {
        update_statements.push(format!(
            "UPDATE \"{table_name}\"
            SET haex_column_hlcs = json_set(haex_column_hlcs, '$.{col}', NEW.\"{HLC_TIMESTAMP_COLUMN}\")
            WHERE {pk_where} AND NEW.\"{col}\" IS NOT OLD.\"{col}\";"
        ));
    }

    let all_updates = update_statements.join("\n            ");

    // Generate condition: at least one tracked column must have changed
    // This prevents marking the table as dirty when only sync metadata columns changed
    let any_tracked_changed: String = if cols_to_track.is_empty() {
        // No columns to track - never mark as dirty from updates
        "0".to_string()
    } else {
        cols_to_track
            .iter()
            .map(|col| format!("NEW.\"{col}\" IS NOT OLD.\"{col}\""))
            .collect::<Vec<_>>()
            .join(" OR ")
    };

    format!(
        "CREATE TRIGGER IF NOT EXISTS \"{trigger_name}\"
            AFTER UPDATE ON \"{table_name}\"
            FOR EACH ROW
            WHEN NEW.{HLC_TIMESTAMP_COLUMN} IS NOT NULL
                AND (SELECT COALESCE(value, '1') FROM {TABLE_CRDT_CONFIGS} WHERE key = 'triggers_enabled') = '1'
            BEGIN
            {all_updates}

            -- Only mark as dirty if at least one tracked column changed
            INSERT OR REPLACE INTO {TABLE_CRDT_DIRTY_TABLES} (table_name, last_modified)
            SELECT '{table_name}', datetime('now')
            WHERE ({any_tracked_changed});
            END;"
    )
}

/// Generates SQL for BEFORE-DELETE trigger.
///
/// Two things happen in one trigger:
/// 1. A row is appended to `haex_deleted_rows` — with a fresh uuid as id, the
///    table name, the deleted row's PKs as a JSON object, and the current
///    transaction HLC. This is the sync-visible "delete event".
/// 2. The table is marked dirty so the scanner picks up the haex_deleted_rows
///    change on the next sync cycle.
///
/// Both are gated by `triggers_enabled` so the sync-receive path can bulk-delete
/// without re-logging.
fn generate_delete_trigger_sql(table_name: &str, pks: &[String]) -> String {
    let trigger_name = DELETE_TRIGGER_TPL.replace("{TABLE_NAME}", table_name);

    // Build JSON object for row_pks: json_object('pk1', OLD."pk1", ...)
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
            INSERT INTO {DELETED_ROWS_TABLE} (id, table_name, row_pks, {HLC_TIMESTAMP_COLUMN}, {COLUMN_HLCS_COLUMN})
            VALUES ({UUID_FUNCTION_NAME}(), '{table_name}', json_object({row_pks_json}), {HLC_FUNCTION_NAME}(), '{{}}');
            INSERT OR REPLACE INTO {TABLE_CRDT_DIRTY_TABLES} (table_name, last_modified)
            VALUES ('{DELETED_ROWS_TABLE}', datetime('now'));
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

/// Ensures that a table has all required CRDT columns.
/// If columns are missing, they are added via ALTER TABLE.
/// Returns true if any columns were added, false if all columns already existed.
pub fn ensure_crdt_columns(tx: &Transaction, table_name: &str) -> Result<bool, CrdtSetupError> {
    let columns = get_table_schema(tx, table_name)?;

    if columns.is_empty() {
        // Table doesn't exist - nothing to do
        return Ok(false);
    }

    let has_hlc = columns.iter().any(|c| c.name == HLC_TIMESTAMP_COLUMN);
    let has_column_hlcs = columns.iter().any(|c| c.name == COLUMN_HLCS_COLUMN);
    let has_column_sigs = columns.iter().any(|c| c.name == COLUMN_SIGS_COLUMN);

    let mut added_any = false;

    if !has_hlc {
        let sql = format!(
            "ALTER TABLE \"{}\" ADD COLUMN \"{}\" TEXT",
            table_name, HLC_TIMESTAMP_COLUMN
        );
        tx.execute(&sql, [])
            .map_err(CrdtSetupError::DatabaseError)?;
        println!(
            "[CRDT] Added missing column '{}' to table '{}'",
            HLC_TIMESTAMP_COLUMN, table_name
        );
        added_any = true;
    }

    if !has_column_hlcs {
        let sql = format!(
            "ALTER TABLE \"{}\" ADD COLUMN \"{}\" TEXT NOT NULL DEFAULT '{{}}'",
            table_name, COLUMN_HLCS_COLUMN
        );
        tx.execute(&sql, [])
            .map_err(CrdtSetupError::DatabaseError)?;
        println!(
            "[CRDT] Added missing column '{}' to table '{}'",
            COLUMN_HLCS_COLUMN, table_name
        );
        added_any = true;
    }

    if !has_column_sigs {
        let sql = format!(
            "ALTER TABLE \"{}\" ADD COLUMN \"{}\" TEXT NOT NULL DEFAULT '{{}}'",
            table_name, COLUMN_SIGS_COLUMN
        );
        tx.execute(&sql, [])
            .map_err(CrdtSetupError::DatabaseError)?;
        println!(
            "[CRDT] Added missing column '{}' to table '{}'",
            COLUMN_SIGS_COLUMN, table_name
        );
        added_any = true;
    }

    Ok(added_any)
}

/// Ensures that a table has all required CRDT columns AND triggers.
/// This is a combined operation that:
/// 1. Adds missing CRDT columns (haex_hlc, haex_column_hlcs)
/// 2. Sets up dirty-table triggers if missing
///
/// Returns (columns_added, triggers_created) tuple.
pub fn ensure_crdt_columns_and_triggers(
    tx: &Transaction,
    table_name: &str,
) -> Result<(bool, bool), CrdtSetupError> {
    // First, ensure CRDT columns exist
    let columns_added = ensure_crdt_columns(tx, table_name)?;

    // Now check if triggers already exist
    let trigger_name = format!("z_dirty_{}_insert", table_name);
    let has_trigger: bool = tx
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'trigger' AND name = ?",
            [&trigger_name],
            |row| row.get(0),
        )
        .unwrap_or(false);

    let triggers_created = if !has_trigger {
        // Setup triggers (this requires CRDT columns to exist)
        match setup_triggers_for_table(tx, table_name, false) {
            Ok(TriggerSetupResult::Success) => {
                println!("[CRDT] Created triggers for table '{}'", table_name);
                true
            }
            Ok(TriggerSetupResult::TableNotFound) => false,
            Err(e) => {
                eprintln!(
                    "[CRDT] Failed to create triggers for '{}': {}",
                    table_name, e
                );
                return Err(e);
            }
        }
    } else {
        false
    };

    Ok((columns_added, triggers_created))
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
    // `haex_deleted_rows` trigger installed by `setup_triggers_for_table`.
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
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                 haex_column_sigs TEXT NOT NULL DEFAULT '{}'
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
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                 haex_column_sigs TEXT NOT NULL DEFAULT '{}'
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
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                 haex_column_sigs TEXT NOT NULL DEFAULT '{}'
             );",
        )
        .unwrap();

        let tx = conn.unchecked_transaction().unwrap();
        setup_triggers_for_table(&tx, "haex_shared_space_sync", false).expect("register triggers");
        tx.commit().unwrap();

        conn
    }

    #[test]
    fn deleting_register_entry_writes_shared_space_delete_log_row() {
        let conn = setup_register_delete_fixture();

        // Seed a register row saying "table T row {id:R} shared into SPACE_X".
        conn.execute(
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc)
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
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc)
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

    #[test]
    fn test_ensure_crdt_columns_nonexistent_table() {
        let conn = Connection::open_in_memory().unwrap();
        let tx = conn.unchecked_transaction().unwrap();

        // Should return false for non-existent table
        let result = ensure_crdt_columns(&tx, "nonexistent_table").unwrap();
        assert!(!result, "Should return false for non-existent table");
    }
}

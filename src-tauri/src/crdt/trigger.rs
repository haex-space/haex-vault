// src-tauri/src/crdt/trigger.rs
//
// New approach: Instead of logging changes to haex_crdt_changes table,
// we just mark tables as "dirty" in haex_crdt_dirty_tables.
// Actual sync happens by scanning the dirty tables directly.
use crate::table_names::TABLE_CRDT_CONFIGS;
use rusqlite::{Connection, Result as RusqliteResult, Row, Transaction};
use serde::Serialize;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use ts_rs::TS;

// Trigger names for dirty table tracking
const INSERT_TRIGGER_TPL: &str = "z_dirty_{TABLE_NAME}_insert";
const UPDATE_TRIGGER_TPL: &str = "z_dirty_{TABLE_NAME}_update";
const DELETE_TRIGGER_TPL: &str = "z_dirty_{TABLE_NAME}_delete";

pub const HLC_TIMESTAMP_COLUMN: &str = "haex_timestamp";
pub const COLUMN_HLCS_COLUMN: &str = "haex_column_hlcs";
pub const TOMBSTONE_COLUMN: &str = "haex_tombstone";

/// Name der custom UUID-Generierungs-Funktion (registriert in database::core::open_and_init_db)
pub const UUID_FUNCTION_NAME: &str = "gen_uuid";

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
    pub is_pk: bool,
}

impl ColumnInfo {
    pub fn from_row(row: &Row) -> RusqliteResult<Self> {
        Ok(ColumnInfo {
            name: row.get("name")?,
            is_pk: row.get::<_, i64>("pk")? > 0,
        })
    }
}

fn is_safe_identifier(name: &str) -> bool {
    // Allow alphanumeric characters, underscores, and hyphens (for extension names like "nuxt-app")
    !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
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

    // Calculate columns to track: all columns EXCEPT PKs, haex_timestamp, and haex_column_hlcs
    let cols_to_track: Vec<String> = columns
        .iter()
        .filter(|c| {
            !c.is_pk
                && c.name != HLC_TIMESTAMP_COLUMN
                && c.name != "haex_column_hlcs"
        })
        .map(|c| c.name.clone())
        .collect();

    let insert_trigger_sql = generate_insert_trigger_sql(table_name, &cols_to_track);
    let update_trigger_sql = generate_update_trigger_sql(table_name, &cols_to_track);
    let delete_trigger_sql = generate_delete_trigger_sql(table_name);

    if recreate {
        drop_triggers_for_table(tx, table_name)?;
    }

    tx.execute_batch(&insert_trigger_sql)?;
    tx.execute_batch(&update_trigger_sql)?;
    tx.execute_batch(&delete_trigger_sql)?;

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

    let sql_batch = format!(
        "{drop_insert_trigger_sql}\n{drop_update_trigger_sql}\n{drop_delete_trigger_sql}"
    );

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
fn generate_insert_trigger_sql(table_name: &str, cols_to_track: &[String]) -> String {
    let trigger_name = INSERT_TRIGGER_TPL.replace("{TABLE_NAME}", table_name);

    // Generate JSON object for haex_column_hlcs with all tracked columns
    let json_pairs: Vec<String> = cols_to_track
        .iter()
        .map(|col| format!("'{}', NEW.{}", col, HLC_TIMESTAMP_COLUMN))
        .collect();
    let json_object = if json_pairs.is_empty() {
        "'{}'".to_string()
    } else {
        format!("json_object({})", json_pairs.join(", "))
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
            WHERE rowid = NEW.rowid;

            INSERT OR REPLACE INTO haex_crdt_dirty_tables (table_name, last_modified)
            VALUES ('{table_name}', datetime('now'));
            END;"
    )
}

/// Generiert das SQL zum Löschen eines Triggers.
fn drop_trigger_sql(trigger_name: String) -> String {
    format!("DROP TRIGGER IF EXISTS \"{trigger_name}\";")
}

/// Generates SQL for UPDATE trigger - updates column HLCs and marks table as dirty
fn generate_update_trigger_sql(table_name: &str, cols_to_track: &[String]) -> String {
    let trigger_name = UPDATE_TRIGGER_TPL.replace("{TABLE_NAME}", table_name);

    // Generate UPDATE statements for each changed column
    // We check each column individually and update its HLC timestamp if it changed
    let mut update_statements: Vec<String> = Vec::new();

    for col in cols_to_track {
        update_statements.push(format!(
            "UPDATE \"{table_name}\"
            SET haex_column_hlcs = json_set(haex_column_hlcs, '$.{col}', NEW.{HLC_TIMESTAMP_COLUMN})
            WHERE rowid = NEW.rowid AND NEW.{col} IS NOT OLD.{col};"
        ));
    }

    let all_updates = update_statements.join("\n            ");

    format!(
        "CREATE TRIGGER IF NOT EXISTS \"{trigger_name}\"
            AFTER UPDATE ON \"{table_name}\"
            FOR EACH ROW
            WHEN NEW.{HLC_TIMESTAMP_COLUMN} IS NOT NULL
                AND (SELECT COALESCE(value, '1') FROM {TABLE_CRDT_CONFIGS} WHERE key = 'triggers_enabled') = '1'
            BEGIN
            {all_updates}

            INSERT OR REPLACE INTO haex_crdt_dirty_tables (table_name, last_modified)
            VALUES ('{table_name}', datetime('now'));
            END;"
    )
}

/// Generates SQL for DELETE trigger - marks table as dirty for sync
fn generate_delete_trigger_sql(table_name: &str) -> String {
    let trigger_name = DELETE_TRIGGER_TPL.replace("{TABLE_NAME}", table_name);

    format!(
        "CREATE TRIGGER IF NOT EXISTS \"{trigger_name}\"
            BEFORE DELETE ON \"{table_name}\"
            FOR EACH ROW
            WHEN (SELECT COALESCE(value, '1') FROM {TABLE_CRDT_CONFIGS} WHERE key = 'triggers_enabled') = '1'
            BEGIN
            INSERT OR REPLACE INTO haex_crdt_dirty_tables (table_name, last_modified)
            VALUES ('{table_name}', datetime('now'));
            END;"
    )
}

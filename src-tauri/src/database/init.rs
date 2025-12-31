// src-tauri/src/database/init.rs
// Database initialization utilities (trigger setup, etc.)

use crate::crdt::trigger;
use crate::database::error::DatabaseError;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_VAULT_SETTINGS};
use rusqlite::{params, Connection};

/// Scans the database for all tables that have a 'haex_tombstone' column
/// These are the tables that need CRDT triggers
pub fn discover_crdt_tables(conn: &Connection) -> Result<Vec<String>, DatabaseError> {
    let mut stmt = conn.prepare(
        "SELECT m.name as table_name
         FROM sqlite_master m
         JOIN pragma_table_info(m.name) p
         WHERE m.type = 'table'
           AND p.name = 'haex_tombstone'
         GROUP BY m.name
         ORDER BY m.name",
    )?;

    let tables: Result<Vec<String>, _> = stmt.query_map([], |row| row.get(0))?.collect();

    Ok(tables?)
}

/// Prüft ob Trigger bereits initialisiert wurden und erstellt sie falls nötig
///
/// Diese Funktion wird beim ersten Öffnen einer Template-DB aufgerufen.
/// Sie erstellt alle CRDT-Trigger für die definierten Tabellen und markiert
/// die Initialisierung in haex_settings.
///
/// Bei Migrations (ALTER TABLE) werden Trigger automatisch neu erstellt,
/// daher ist kein Versioning nötig.
pub fn ensure_triggers_initialized(conn: &mut Connection) -> Result<bool, DatabaseError> {
    let tx = conn.transaction()?;

    // Check if triggers already initialized
    let check_sql = format!("SELECT value FROM {TABLE_VAULT_SETTINGS} WHERE key = ? AND type = ?");
    let initialized: Option<String> = tx
        .query_row(
            &check_sql,
            params!["triggers_initialized", "system"],
            |row| row.get(0),
        )
        .ok();

    if initialized.is_some() {
        eprintln!("DEBUG: Triggers already initialized, skipping");
        tx.commit()?; // Wichtig: Transaktion trotzdem abschließen
        return Ok(true); // true = war schon initialisiert
    }

    eprintln!("INFO: Initializing CRDT triggers for database...");

    // Discover all tables with haex_tombstone column
    let crdt_tables = discover_crdt_tables(&tx)?;
    eprintln!("INFO: Discovered {} CRDT tables", crdt_tables.len());

    // Initialize triggers_enabled config (enable triggers by default)
    eprintln!("INFO: Initializing triggers_enabled config...");
    tx.execute(
        &format!(
            "INSERT OR REPLACE INTO {TABLE_CRDT_CONFIGS} (key, type, value) VALUES ('triggers_enabled', 'system', '1')"
        ),
        [],
    )?;

    // Create triggers for all discovered CRDT tables
    for table_name in crdt_tables {
        eprintln!("  - Setting up triggers for: {table_name}");
        trigger::setup_triggers_for_table(&tx, &table_name, false)?;
    }

    tx.commit()?;
    eprintln!("INFO: ✓ CRDT triggers created successfully (flag pending)");
    Ok(false) // false = wurde gerade initialisiert
}

/// Ensures all CRDT tables have proper triggers set up.
/// This is called after applying synced extension migrations to make sure
/// newly created extension tables have their dirty-table triggers.
///
/// Unlike ensure_triggers_initialized(), this function:
/// - Does NOT check/set the triggers_initialized flag
/// - Sets up triggers for any table that's missing them
/// - Is idempotent (can be called multiple times safely)
pub fn ensure_triggers_for_all_tables(conn: &mut Connection) -> Result<usize, DatabaseError> {
    let tx = conn.transaction()?;

    // Discover all tables with haex_tombstone column
    let crdt_tables = discover_crdt_tables(&tx)?;
    let mut triggers_created = 0;

    for table_name in &crdt_tables {
        // Check if this table already has dirty-table triggers
        let trigger_name = format!("z_dirty_{}_insert", table_name);
        let has_trigger: bool = tx
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'trigger' AND name = ?",
                [&trigger_name],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_trigger {
            eprintln!(
                "[SYNC] Setting up missing CRDT triggers for table: {}",
                table_name
            );
            trigger::setup_triggers_for_table(&tx, table_name, false)?;
            triggers_created += 1;
        }
    }

    tx.commit()?;

    if triggers_created > 0 {
        eprintln!(
            "[SYNC] ✓ Created triggers for {} extension tables",
            triggers_created
        );
    }

    Ok(triggers_created)
}

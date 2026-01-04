// src-tauri/src/database/init.rs
// Database initialization utilities (trigger setup, etc.)

use crate::crdt::trigger;
use crate::database::error::DatabaseError;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_VAULT_SETTINGS};
use rusqlite::{params, Connection};
use uuid::Uuid;

/// Current version of the CRDT trigger logic.
/// Increment this whenever the trigger generation code changes significantly.
/// This forces all existing vaults to recreate their triggers on next open.
///
/// Version history:
/// - 1: Initial trigger implementation
/// - 2: Fix sync loop: UPDATE trigger only marks table dirty if tracked columns changed
/// - 3: Track haex_tombstone column to enable proper sync of soft-deletes
const TRIGGER_VERSION: i32 = 3;

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
/// If the trigger version has changed, triggers are recreated to apply fixes.
pub fn ensure_triggers_initialized(conn: &mut Connection) -> Result<bool, DatabaseError> {
    let tx = conn.transaction()?;

    // Check if triggers already initialized and get version
    let check_sql = format!("SELECT value FROM {TABLE_VAULT_SETTINGS} WHERE key = ? AND type = ?");
    let current_version: Option<i32> = tx
        .query_row(
            &check_sql,
            params!["trigger_version", "system"],
            |row| {
                let val: String = row.get(0)?;
                Ok(val.parse().unwrap_or(1))
            },
        )
        .ok();

    // Also check old flag for backwards compatibility
    let initialized: Option<String> = tx
        .query_row(
            &check_sql,
            params!["triggers_initialized", "system"],
            |row| row.get(0),
        )
        .ok();

    let needs_update = match current_version {
        Some(v) if v >= TRIGGER_VERSION => {
            eprintln!("DEBUG: Triggers at version {v}, current is {TRIGGER_VERSION}, skipping");
            tx.commit()?;
            return Ok(true);
        }
        Some(v) => {
            eprintln!("INFO: Trigger version {v} < {TRIGGER_VERSION}, recreating triggers...");
            true
        }
        None if initialized.is_some() => {
            eprintln!("INFO: Triggers initialized but no version, upgrading to v{TRIGGER_VERSION}...");
            true
        }
        None => {
            eprintln!("INFO: First-time trigger initialization (v{TRIGGER_VERSION})...");
            false
        }
    };

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

    // Create/recreate triggers for all discovered CRDT tables
    for table_name in crdt_tables {
        eprintln!("  - Setting up triggers for: {table_name}");
        // Use recreate=true if we need to update existing triggers
        trigger::setup_triggers_for_table(&tx, &table_name, needs_update)?;
    }

    // Store trigger version
    // Check if entry exists first, then INSERT or UPDATE
    // Note: We can't use ON CONFLICT because the UNIQUE index is partial (WHERE haex_tombstone = 0)
    // and SQLite's ON CONFLICT doesn't work with partial indexes
    let existing: Option<String> = tx
        .query_row(
            &format!(
                "SELECT id FROM {TABLE_VAULT_SETTINGS} WHERE key = 'trigger_version' AND type = 'system' AND haex_tombstone = 0"
            ),
            [],
            |row| row.get(0),
        )
        .ok();

    if let Some(existing_id) = existing {
        tx.execute(
            &format!("UPDATE {TABLE_VAULT_SETTINGS} SET value = ? WHERE id = ?"),
            params![TRIGGER_VERSION.to_string(), existing_id],
        )?;
    } else {
        let new_id = Uuid::new_v4().to_string();
        tx.execute(
            &format!(
                "INSERT INTO {TABLE_VAULT_SETTINGS} (id, key, type, value, haex_tombstone) VALUES (?, 'trigger_version', 'system', ?, 0)"
            ),
            params![new_id, TRIGGER_VERSION.to_string()],
        )?;
    }

    tx.commit()?;
    eprintln!("INFO: ✓ CRDT triggers at version {TRIGGER_VERSION}");
    Ok(false) // false = wurde gerade initialisiert/aktualisiert
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

// src-tauri/src/database/migrations/query.rs
// State queries: applied / unapplied / all migrations.

use super::apply::{get_unapplied_migrations_internal, migrations_table_exists};
use super::load::load_bundled_migrations;
use super::MigrationInfo;
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::database::generated::HaexCrdtMigrationsNoSync;
use crate::table_names::TABLE_CRDT_MIGRATIONS;
use crate::AppState;
use rusqlite::Connection;
use tauri::State;

/// Gets the list of applied migrations with their details
/// Returns empty Vec if migrations table doesn't exist yet
#[tauri::command]
pub fn get_applied_core_migrations(
    state: State<'_, AppState>,
) -> Result<Vec<HaexCrdtMigrationsNoSync>, DatabaseError> {
    println!("[MIGRATIONS] get_applied_core_migrations called");

    with_connection(&state.db, |conn| {
        let exists = migrations_table_exists(conn)?;
        println!(
            "[MIGRATIONS] get_applied_core_migrations: table exists = {}",
            exists
        );

        if !exists {
            println!("[MIGRATIONS] get_applied_core_migrations: returning empty vec (table doesn't exist)");
            return Ok(Vec::new());
        }

        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, extension_id, migration_name, migration_content, applied_at FROM {TABLE_CRDT_MIGRATIONS} ORDER BY applied_at"
            ))
            .map_err(DatabaseError::from)?;

        let migrations = stmt
            .query_map([], |row| {
                Ok(HaexCrdtMigrationsNoSync {
                    id: row.get(0)?,
                    extension_id: row.get(1)?,
                    migration_name: row.get(2)?,
                    migration_content: row.get(3)?,
                    applied_at: row.get(4)?,
                })
            })
            .map_err(DatabaseError::from)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::from)?;

        println!(
            "[MIGRATIONS] get_applied_core_migrations: found {} migrations",
            migrations.len()
        );
        Ok(migrations)
    })
}

/// Gets the list of unapplied (pending) migrations
#[tauri::command]
pub fn get_unapplied_core_migrations(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<MigrationInfo>, DatabaseError> {
    println!("[MIGRATIONS] get_unapplied_core_migrations called");
    with_connection(&state.db, |conn| {
        get_unapplied_migrations_internal(&app_handle, conn)
    })
}

/// Gets all migrations (both applied and unapplied) with their status
#[tauri::command]
pub fn get_all_core_migrations(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<MigrationInfo>, DatabaseError> {
    println!("[MIGRATIONS] get_all_core_migrations called");

    with_connection(&state.db, |conn| {
        // Load all migration files
        let all_migrations = load_bundled_migrations(&app_handle)?;

        // Get applied migrations
        let applied = get_applied_migrations_map(conn)?;

        // Combine into MigrationInfo structs
        let mut migrations: Vec<MigrationInfo> = all_migrations
            .into_iter()
            .map(|(name, content)| {
                let applied_at = applied.get(&name).cloned();
                MigrationInfo {
                    migration_name: name,
                    migration_content: content,
                    is_applied: applied_at.is_some(),
                    applied_at,
                }
            })
            .collect();

        // Sort by migration name
        migrations.sort_by(|a, b| a.migration_name.cmp(&b.migration_name));

        Ok(migrations)
    })
}

/// Gets the list of already applied migration names only
/// Returns empty Vec if migrations table doesn't exist yet
pub(super) fn get_applied_migration_names(conn: &Connection) -> Result<Vec<String>, DatabaseError> {
    println!("[MIGRATIONS] get_applied_migration_names: checking if table exists...");

    if !migrations_table_exists(conn)? {
        println!(
            "[MIGRATIONS] get_applied_migration_names: table doesn't exist, returning empty vec"
        );
        return Ok(Vec::new());
    }

    println!("[MIGRATIONS] get_applied_migration_names: querying applied migrations...");

    let mut stmt = conn
        .prepare(&format!(
            "SELECT migration_name FROM {TABLE_CRDT_MIGRATIONS} ORDER BY applied_at"
        ))
        .map_err(DatabaseError::from)?;

    let migrations = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(DatabaseError::from)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from)?;

    println!(
        "[MIGRATIONS] get_applied_migration_names: found {} applied migrations",
        migrations.len()
    );
    Ok(migrations)
}

/// Gets applied migrations as a HashMap (name -> applied_at timestamp)
/// Returns empty HashMap if migrations table doesn't exist yet
fn get_applied_migrations_map(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, String>, DatabaseError> {
    println!("[MIGRATIONS] get_applied_migrations_map: checking if table exists...");

    if !migrations_table_exists(conn)? {
        println!(
            "[MIGRATIONS] get_applied_migrations_map: table doesn't exist, returning empty map"
        );
        return Ok(std::collections::HashMap::new());
    }

    let mut stmt = conn
        .prepare(&format!(
            "SELECT migration_name, applied_at FROM {TABLE_CRDT_MIGRATIONS}"
        ))
        .map_err(DatabaseError::from)?;

    let migrations = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(DatabaseError::from)?
        .collect::<Result<std::collections::HashMap<_, _>, _>>()
        .map_err(DatabaseError::from)?;

    println!(
        "[MIGRATIONS] get_applied_migrations_map: found {} entries",
        migrations.len()
    );
    Ok(migrations)
}

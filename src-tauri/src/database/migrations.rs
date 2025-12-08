// src-tauri/src/database/migrations.rs
// Core migration system for system tables

use crate::database::core::{with_connection, DRIZZLE_STATEMENT_BREAKPOINT};
use crate::database::error::DatabaseError;
use crate::database::generated::HaexCrdtMigrations;
use crate::table_names::TABLE_CRDT_MIGRATIONS;
use crate::AppState;
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{path::BaseDirectory, Manager, State};
use tauri_plugin_fs::FsExt;

/// List of all migration file names (without .sql extension)
/// When adding new migrations, append them to this list
const MIGRATION_FILES: &[&str] = &[
    "0000_harsh_madripoor",
    "0001_hard_turbo",
    "0002_wandering_lily_hollister",
    "0003_loud_ulik",
    "0004_nappy_mother_askani",
    "0005_bitter_lily_hollister",
    "0006_short_madame_hydra",
];

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MigrationInfo {
    pub migration_name: String,
    pub migration_content: String,
    pub is_applied: bool,
    pub applied_at: Option<String>,
}

/// Applies all pending core migrations from the bundled migration files
///
/// This function:
/// 1. Ensures the haex_crdt_migrations table exists (bootstrapping)
/// 2. Reads all migration files from the bundled migrations directory
/// 3. Checks which migrations have already been applied
/// 4. Applies pending migrations in order
/// 5. Records each successful migration in haex_crdt_migrations
///
/// # Arguments
/// * `app_handle` - Tauri app handle to resolve bundled resource paths
/// * `state` - App state containing the database connection
///
/// # Returns
/// * `Ok(usize)` - Number of migrations applied
/// * `Err(DatabaseError)` - If any migration fails
#[tauri::command]
pub fn apply_core_migrations(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<usize, DatabaseError> {
    println!("[MIGRATIONS] ========== apply_core_migrations START ==========");

    with_connection(&state.db, |conn| {
        // Check if migrations table exists before we start
        let table_exists_before = migrations_table_exists(conn)?;
        println!(
            "[MIGRATIONS] migrations_table_exists BEFORE applying: {}",
            table_exists_before
        );

        // Step 1: Get unapplied migrations
        // Note: If migrations table doesn't exist yet, all migrations are considered unapplied
        println!("[MIGRATIONS] Getting unapplied migrations...");
        let unapplied = get_unapplied_migrations_internal(&app_handle, conn)?;
        println!(
            "[MIGRATIONS] Found {} unapplied migrations: {:?}",
            unapplied.len(),
            unapplied
                .iter()
                .map(|m| &m.migration_name)
                .collect::<Vec<_>>()
        );

        if unapplied.is_empty() {
            println!("[MIGRATIONS] ✅ No pending core migrations");
            return Ok(0);
        }

        println!(
            "[MIGRATIONS] 📦 Applying {} core migrations...",
            unapplied.len()
        );

        // Step 2: Apply each pending migration
        let mut applied_count = 0;
        for migration in unapplied {
            println!(
                "[MIGRATIONS] Applying migration {} of total...",
                applied_count + 1
            );
            apply_single_migration(conn, &migration.migration_name, &migration.migration_content)?;
            applied_count += 1;
            println!(
                "[MIGRATIONS] Migration '{}' applied successfully",
                migration.migration_name
            );
        }

        // Check if migrations table exists after we're done
        let table_exists_after = migrations_table_exists(conn)?;
        println!(
            "[MIGRATIONS] migrations_table_exists AFTER applying: {}",
            table_exists_after
        );

        // List all tables for debugging
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        println!("[MIGRATIONS] All tables in database: {:?}", tables);

        println!(
            "[MIGRATIONS] ✅ Successfully applied {} core migrations",
            applied_count
        );
        println!("[MIGRATIONS] ========== apply_core_migrations END ==========");
        Ok(applied_count)
    })
}

/// Gets the list of applied migrations with their details
/// Returns empty Vec if migrations table doesn't exist yet
#[tauri::command]
pub fn get_applied_core_migrations(
    state: State<'_, AppState>,
) -> Result<Vec<HaexCrdtMigrations>, DatabaseError> {
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
                "SELECT id, migration_name, migration_content, applied_at FROM {TABLE_CRDT_MIGRATIONS} ORDER BY applied_at"
            ))
            .map_err(DatabaseError::from)?;

        let migrations = stmt
            .query_map([], |row| {
                Ok(HaexCrdtMigrations {
                    id: row.get(0)?,
                    migration_name: row.get(1)?,
                    migration_content: row.get(2)?,
                    applied_at: row.get(3)?,
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

// ===== Internal Helper Functions =====

/// Internal function to get unapplied migrations (requires Connection)
fn get_unapplied_migrations_internal(
    app_handle: &tauri::AppHandle,
    conn: &Connection,
) -> Result<Vec<MigrationInfo>, DatabaseError> {
    println!("[MIGRATIONS] get_unapplied_migrations_internal: loading bundled migrations...");
    let all_migrations = load_bundled_migrations(app_handle)?;
    println!(
        "[MIGRATIONS] get_unapplied_migrations_internal: found {} bundled migrations",
        all_migrations.len()
    );

    println!("[MIGRATIONS] get_unapplied_migrations_internal: getting applied migration names...");
    let applied_names = get_applied_migration_names(conn)?;
    println!(
        "[MIGRATIONS] get_unapplied_migrations_internal: {} migrations already applied: {:?}",
        applied_names.len(),
        applied_names
    );

    let unapplied: Vec<MigrationInfo> = all_migrations
        .into_iter()
        .filter(|(name, _)| !applied_names.contains(name))
        .map(|(name, content)| MigrationInfo {
            migration_name: name,
            migration_content: content,
            is_applied: false,
            applied_at: None,
        })
        .collect();

    println!(
        "[MIGRATIONS] get_unapplied_migrations_internal: {} migrations to apply",
        unapplied.len()
    );
    Ok(unapplied)
}

/// Checks if the migrations table exists
///
/// The migrations table is created by the first Drizzle migration.
/// This function checks if it exists to determine if any migrations have been applied.
fn migrations_table_exists(conn: &Connection) -> Result<bool, DatabaseError> {
    println!(
        "[MIGRATIONS] migrations_table_exists: checking for table '{}'",
        TABLE_CRDT_MIGRATIONS
    );

    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)",
            [TABLE_CRDT_MIGRATIONS],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from)?;

    println!("[MIGRATIONS] migrations_table_exists: result = {}", exists);
    Ok(exists)
}

/// Loads all migration files from the bundled resources directory
///
/// Uses FsExt to read files, which works on all platforms including Android
/// where resources are stored in the APK as assets.
fn load_bundled_migrations(
    app_handle: &tauri::AppHandle,
) -> Result<Vec<(String, String)>, DatabaseError> {
    println!(
        "[MIGRATIONS] load_bundled_migrations: loading {} migrations via FsExt",
        MIGRATION_FILES.len()
    );

    let fs = app_handle.fs();
    let mut migrations = Vec::new();

    for migration_name in MIGRATION_FILES {
        let relative_path = format!("database/migrations/{}.sql", migration_name);

        let resource_path = app_handle
            .path()
            .resolve(&relative_path, BaseDirectory::Resource)
            .map_err(|e| DatabaseError::MigrationError {
                reason: format!(
                    "Failed to resolve migration path '{}': {}",
                    relative_path, e
                ),
            })?;

        println!(
            "[MIGRATIONS] load_bundled_migrations: reading '{}' from {:?}",
            migration_name, resource_path
        );

        let content = fs
            .read_to_string(&resource_path)
            .map_err(|e| DatabaseError::MigrationError {
                reason: format!(
                    "Failed to read migration file '{}': {}",
                    migration_name, e
                ),
            })?;

        migrations.push((migration_name.to_string(), content));
    }

    println!(
        "[MIGRATIONS] 📂 Loaded {} migration files: {:?}",
        migrations.len(),
        migrations.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    Ok(migrations)
}

/// Gets the list of already applied migration names only
/// Returns empty Vec if migrations table doesn't exist yet
fn get_applied_migration_names(conn: &Connection) -> Result<Vec<String>, DatabaseError> {
    println!("[MIGRATIONS] get_applied_migration_names: checking if table exists...");

    if !migrations_table_exists(conn)? {
        println!("[MIGRATIONS] get_applied_migration_names: table doesn't exist, returning empty vec");
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

/// Applies a single migration file
///
/// The migration file may contain multiple SQL statements separated by
/// '--> statement-breakpoint' markers (Drizzle format)
fn apply_single_migration(
    conn: &mut Connection,
    migration_name: &str,
    migration_content: &str,
) -> Result<(), DatabaseError> {
    println!(
        "[MIGRATIONS] apply_single_migration: START '{}'",
        migration_name
    );

    // Start a transaction
    println!("[MIGRATIONS] apply_single_migration: starting transaction...");
    let tx = conn.transaction().map_err(DatabaseError::from)?;

    // Split migration content by statement breakpoint
    let statements: Vec<&str> = migration_content
        .split(DRIZZLE_STATEMENT_BREAKPOINT)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    println!(
        "[MIGRATIONS] apply_single_migration: {} SQL statements found",
        statements.len()
    );

    // Execute each statement
    for (idx, statement) in statements.iter().enumerate() {
        println!(
            "[MIGRATIONS] apply_single_migration: executing statement {}/{}",
            idx + 1,
            statements.len()
        );
        // Log first 100 chars of statement for debugging
        let stmt_preview: String = statement.chars().take(100).collect();
        println!(
            "[MIGRATIONS] apply_single_migration: statement preview: {}...",
            stmt_preview
        );

        tx.execute(statement, [])
            .map_err(|e| DatabaseError::MigrationError {
                reason: format!(
                    "Failed to execute statement {} in migration '{}': {}. Statement: {}",
                    idx + 1,
                    migration_name,
                    e,
                    statement
                ),
            })?;

        println!(
            "[MIGRATIONS] apply_single_migration: statement {} executed successfully",
            idx + 1
        );
    }

    // Check if migrations table was created by this migration
    println!("[MIGRATIONS] apply_single_migration: checking if migrations table exists now...");
    let table_exists: bool = tx
        .query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='{}')",
                TABLE_CRDT_MIGRATIONS
            ),
            [],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from)?;
    println!(
        "[MIGRATIONS] apply_single_migration: migrations table exists = {}",
        table_exists
    );

    // Record the migration as applied
    println!("[MIGRATIONS] apply_single_migration: recording migration in migrations table...");
    let migration_id = uuid::Uuid::new_v4().to_string();

    tx.execute(
        &format!(
            "INSERT INTO {TABLE_CRDT_MIGRATIONS} (id, migration_name, migration_content, applied_at)
             VALUES (?1, ?2, ?3, datetime('now'))"
        ),
        params![migration_id, migration_name, migration_content],
    )
    .map_err(|e| {
        println!(
            "[MIGRATIONS] apply_single_migration: ERROR recording migration: {}",
            e
        );
        DatabaseError::from(e)
    })?;

    println!("[MIGRATIONS] apply_single_migration: migration recorded successfully");

    // Commit the transaction
    println!("[MIGRATIONS] apply_single_migration: committing transaction...");
    tx.commit().map_err(DatabaseError::from)?;

    println!(
        "[MIGRATIONS] apply_single_migration: ✅ '{}' applied successfully",
        migration_name
    );
    Ok(())
}

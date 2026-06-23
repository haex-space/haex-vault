// src-tauri/src/database/migrations/apply.rs
// Migration application: apply_core_migrations + helpers.

use super::load::load_bundled_migrations;
use super::query::get_applied_migration_names;
use super::MigrationInfo;
use crate::crdt::transformer::CrdtTransformer;
use crate::database::core::{with_connection, DRIZZLE_STATEMENT_BREAKPOINT};
use crate::database::error::DatabaseError;
use crate::table_names::TABLE_CRDT_MIGRATIONS;
use crate::AppState;
use rusqlite::{params, Connection};
use tauri::State;

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
            apply_single_migration(
                conn,
                &migration.migration_name,
                &migration.migration_content,
            )?;
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

// ===== Internal Helper Functions =====

/// Internal function to get unapplied migrations (requires Connection)
pub(super) fn get_unapplied_migrations_internal(
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
pub(super) fn migrations_table_exists(conn: &Connection) -> Result<bool, DatabaseError> {
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

    // Create CrdtTransformer for CREATE TABLE transformation
    // This automatically adds CRDT columns to syncable tables
    let transformer = CrdtTransformer::new();

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

        // Transform CREATE TABLE statements to add CRDT columns
        // Other statements pass through unchanged
        let final_sql = transformer
            .transform_ddl_statement(statement)
            .unwrap_or_else(|_| statement.to_string());

        if final_sql != *statement {
            println!(
                "[MIGRATIONS] apply_single_migration: CRDT transformed SQL: {}...",
                final_sql.chars().take(150).collect::<String>()
            );
        }

        tx.execute(&final_sql, [])
            .map_err(|e| DatabaseError::MigrationError {
                reason: format!(
                    "Failed to execute statement {} in migration '{}': {}. Statement: {}",
                    idx + 1,
                    migration_name,
                    e,
                    final_sql
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

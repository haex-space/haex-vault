// src-tauri/src/extension/core/migrations.rs
//
// Extension migration registration and execution.

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::extension::core::manifest::{ExtensionManifest, MigrationJournal};
use crate::extension::core::path_utils::validate_path_in_directory;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::database::{execute_migration_statements, ExtensionSqlContext};
use crate::extension::error::ExtensionError;
use crate::table_names::TABLE_EXTENSION_MIGRATIONS;
use crate::AppState;
use serde_json::Value as JsonValue;
use std::fs;
use std::path::PathBuf;
use tauri::State;

/// Registers and applies migrations from the extension bundle at install time.
///
/// This reads the migrations from the bundle's migrations_dir (specified in manifest),
/// validates them, executes them, and stores them as applied in the database.
///
/// # Arguments
/// * `extension_dir` - Path to the installed extension directory
/// * `manifest` - The extension manifest
/// * `extension_id` - The database ID of the extension
/// * `state` - App state
pub async fn register_bundle_migrations(
    extension_dir: &PathBuf,
    manifest: &ExtensionManifest,
    extension_id: &str,
    state: &State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let migrations_dir = match &manifest.migrations_dir {
        Some(dir) => dir,
        None => {
            eprintln!(
                "[INSTALL_MIGRATIONS] No migrations_dir in manifest for {}::{}",
                manifest.public_key, manifest.name
            );
            return Ok(());
        }
    };

    eprintln!(
        "[INSTALL_MIGRATIONS] Loading migrations from {} for {}::{}",
        migrations_dir, manifest.public_key, manifest.name
    );

    // Validate migrations_dir path to prevent path traversal attacks
    // The migrations directory MUST be within the extension directory
    let _migrations_path =
        validate_path_in_directory(extension_dir, migrations_dir, true)?.ok_or_else(|| {
            ExtensionError::ValidationError {
                reason: format!(
                    "Migrations directory '{}' does not exist or is outside extension directory",
                    migrations_dir
                ),
            }
        })?;

    // Read _journal.json to get migration order
    let journal_relative_path = format!("{}/meta/_journal.json", migrations_dir);
    let journal_path =
        validate_path_in_directory(extension_dir, &journal_relative_path, true)?.ok_or_else(
            || {
                eprintln!(
                    "[INSTALL_MIGRATIONS] No _journal.json found at {}",
                    journal_relative_path
                );
                ExtensionError::ValidationError {
                    reason: format!(
                        "_journal.json not found at {}/meta/_journal.json",
                        migrations_dir
                    ),
                }
            },
        )?;

    let journal_content = fs::read_to_string(&journal_path).map_err(|e| {
        ExtensionError::filesystem_with_path(journal_path.display().to_string(), e)
    })?;

    let journal: MigrationJournal =
        serde_json::from_str(&journal_content).map_err(|e| ExtensionError::ManifestError {
            reason: format!("Failed to parse _journal.json: {}", e),
        })?;

    eprintln!(
        "[INSTALL_MIGRATIONS] Found {} migrations in journal",
        journal.entries.len()
    );

    // Sort entries by idx to ensure correct order
    let mut entries = journal.entries.clone();
    entries.sort_by_key(|e| e.idx);

    // Process each migration in order
    for entry in &entries {
        // Validate SQL file path to prevent path traversal
        let sql_relative_path = format!("{}/{}.sql", migrations_dir, entry.tag);
        let sql_file_path =
            match validate_path_in_directory(extension_dir, &sql_relative_path, true)? {
                Some(path) => path,
                None => {
                    eprintln!(
                        "[INSTALL_MIGRATIONS] SQL file not found: {}",
                        sql_relative_path
                    );
                    continue;
                }
            };

        let sql_content = fs::read_to_string(&sql_file_path).map_err(|e| {
            ExtensionError::filesystem_with_path(sql_file_path.display().to_string(), e)
        })?;

        eprintln!("[INSTALL_MIGRATIONS] Processing migration: {}", entry.tag);

        // Create context for SQL execution
        let ctx = ExtensionSqlContext::new(manifest.public_key.clone(), manifest.name.clone());

        // Execute all statements using the helper function
        // This validates table prefixes and executes with CRDT support
        let stmt_count = execute_migration_statements(&ctx, &sql_content, state.inner())?;

        eprintln!(
            "[INSTALL_MIGRATIONS] Migration '{}' executed ({} statements)",
            entry.tag, stmt_count
        );

        // Store migration as applied in the database
        with_connection(&state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;
            let migration_id = uuid::Uuid::new_v4().to_string();

            let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
                reason: "Failed to lock HLC service".to_string(),
            })?;

            let insert_sql = format!(
                "INSERT OR IGNORE INTO {TABLE_EXTENSION_MIGRATIONS}
                 (id, extension_id, extension_version, migration_name, sql_statement)
                 VALUES (?, ?, ?, ?, ?)"
            );
            let params: Vec<JsonValue> = vec![
                JsonValue::String(migration_id),
                JsonValue::String(extension_id.to_string()),
                JsonValue::String(manifest.version.clone()),
                JsonValue::String(entry.tag.clone()),
                JsonValue::String(sql_content.clone()),
            ];
            SqlExecutor::execute_internal(&tx, &hlc_service, &insert_sql, &params)?;

            tx.commit().map_err(DatabaseError::from)?;
            Ok::<(), DatabaseError>(())
        })?;

        eprintln!(
            "[INSTALL_MIGRATIONS] Migration '{}' applied and stored",
            entry.tag
        );
    }

    eprintln!(
        "[INSTALL_MIGRATIONS] ✅ Completed migration registration for {}::{}",
        manifest.public_key, manifest.name
    );

    Ok(())
}

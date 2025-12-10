// src-tauri/src/database/mod.rs

pub mod core;
pub mod error;
pub mod generated;
pub mod init;
pub mod migrations;
pub mod stats;

use crate::crdt::hlc::HlcService;
use crate::database::core::{execute_with_crdt, with_connection};
use crate::database::error::DatabaseError;
use crate::extension::database::executor::SqlExecutor;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_VAULT_SETTINGS};
use crate::AppState;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::Path;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;
use std::{fs, sync::Arc};
use tauri::{path::BaseDirectory, AppHandle, Emitter, Manager, State};
#[cfg(not(target_os = "android"))]
use trash;
use ts_rs::TS;

pub struct DbConnection(pub Arc<Mutex<Option<Connection>>>);

const VAULT_EXTENSION: &str = ".db";
const VAULT_DIRECTORY: &str = "vaults";

#[tauri::command]
pub fn sql_select(
    sql: String,
    params: Vec<JsonValue>,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    core::select(sql, params, &state.db)
}

#[tauri::command]
pub fn sql_execute(
    sql: String,
    params: Vec<JsonValue>,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    core::execute(sql, params, &state.db)
}

#[tauri::command]
pub fn sql_select_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    core::select_with_crdt(sql, params, &state.db)
}

#[tauri::command]
pub fn sql_execute_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
        reason: "Failed to lock HLC service".to_string(),
    })?;
    let result = core::execute_with_crdt(sql, params, &state.db, &hlc_service)?;

    // Emit event to notify frontend that dirty tables may have changed
    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    Ok(result)
}

#[tauri::command]
pub fn sql_query_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    let result = core::with_connection(&state.db, |conn| {
        let tx = conn.transaction().map_err(DatabaseError::from)?;
        let (_modified_tables, result) =
            SqlExecutor::query_internal(&tx, &hlc_service, &sql, &params)?;
        tx.commit().map_err(DatabaseError::from)?;
        Ok(result)
    })?;

    // Emit event to notify frontend that dirty tables may have changed
    let _ = app_handle.emit("crdt:dirty-tables-changed", ());

    Ok(result)
}

/// Resolves a database name to the full vault path
fn get_vault_path(app_handle: &AppHandle, vault_name: &str) -> Result<String, DatabaseError> {
    // Sicherstellen, dass der Name eine .db Endung hat
    let vault_file_name = if vault_name.ends_with(VAULT_EXTENSION) {
        vault_name.to_string()
    } else {
        format!("{vault_name}{VAULT_EXTENSION}")
    };

    let vault_directory = get_vaults_directory(app_handle)?;

    let vault_path = app_handle
        .path()
        .resolve(
            format!("{vault_directory}/{vault_file_name}"),
            BaseDirectory::AppLocalData,
        )
        .map_err(|e| DatabaseError::PathResolutionError {
            reason: format!("Failed to resolve vault path for '{vault_file_name}': {e}"),
        })?;

    // Sicherstellen, dass das vaults-Verzeichnis existiert
    if let Some(parent) = vault_path.parent() {
        fs::create_dir_all(parent).map_err(|e| DatabaseError::IoError {
            path: parent.display().to_string(),
            reason: format!("Failed to create vaults directory: {e}"),
        })?;
    }

    Ok(vault_path.to_string_lossy().to_string())
}

/// Returns the vaults directory path
#[tauri::command]
pub fn get_vaults_directory(app_handle: &AppHandle) -> Result<String, DatabaseError> {
    let vaults_dir = app_handle
        .path()
        .resolve(VAULT_DIRECTORY, BaseDirectory::AppLocalData)
        .map_err(|e| DatabaseError::PathResolutionError {
            reason: e.to_string(),
        })?;

    Ok(vaults_dir.to_string_lossy().to_string())
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    name: String,
    last_access: u64,
    path: String,
}

/// Lists all vault databases in the vaults directory
#[tauri::command]
pub fn list_vaults(app_handle: AppHandle) -> Result<Vec<VaultInfo>, DatabaseError> {
    let vaults_dir_str = get_vaults_directory(&app_handle)?;
    let vaults_dir = Path::new(&vaults_dir_str);

    println!("Suche vaults in {}", vaults_dir.display());

    let mut vaults: Vec<VaultInfo> = vec![];

    if !vaults_dir.exists() {
        println!("Vaults-Verzeichnis existiert nicht, gebe leere Liste zurück.");
        return Ok(vec![]);
    }

    for entry in fs::read_dir(vaults_dir).map_err(|e| DatabaseError::IoError {
        path: "vaults directory".to_string(),
        reason: e.to_string(),
    })? {
        let entry = entry.map_err(|e| DatabaseError::IoError {
            path: "vaults directory entry".to_string(),
            reason: e.to_string(),
        })?;

        println!("Suche entry {}", entry.path().to_string_lossy());
        let path = entry.path();
        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.ends_with(VAULT_EXTENSION) {
                    // Entferne .db Endung für die Rückgabe
                    println!("Vault gefunden {filename}");

                    let metadata = fs::metadata(&path).map_err(|e| DatabaseError::IoError {
                        path: path.to_string_lossy().to_string(),
                        reason: format!("Metadaten konnten nicht gelesen werden: {e}"),
                    })?;

                    let last_access_timestamp = metadata
                        .accessed()
                        .map_err(|e| DatabaseError::IoError {
                            path: path.to_string_lossy().to_string(),
                            reason: format!("Zugriffszeit konnte nicht gelesen werden: {e}"),
                        })?
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default() // Fallback für den seltenen Fall einer Zeit vor 1970
                        .as_secs();

                    let vault_name = filename.trim_end_matches(VAULT_EXTENSION).to_string();

                    vaults.push(VaultInfo {
                        name: vault_name,
                        last_access: last_access_timestamp,
                        path: path.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    Ok(vaults)
}

/// Checks if a vault with the given name exists
#[tauri::command]
pub fn vault_exists(app_handle: AppHandle, vault_name: String) -> Result<bool, DatabaseError> {
    let vault_path = get_vault_path(&app_handle, &vault_name)?;
    Ok(Path::new(&vault_path).exists())
}

/// Imports a vault database file from an external location into the vaults directory.
/// Returns the new path of the imported vault.
/// Fails if a vault with the same name already exists.
#[tauri::command]
pub fn import_vault(app_handle: AppHandle, source_path: String) -> Result<String, DatabaseError> {
    let source = Path::new(&source_path);

    // Validate source file exists
    if !source.exists() {
        return Err(DatabaseError::IoError {
            path: source_path.clone(),
            reason: "Source file does not exist".to_string(),
        });
    }

    // Validate source file has .db extension
    if source.extension().and_then(|e| e.to_str()) != Some("db") {
        return Err(DatabaseError::ValidationError {
            reason: "Source file must have .db extension".to_string(),
        });
    }

    // Get the file name from the source path
    let file_name = source.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        DatabaseError::ValidationError {
            reason: "Could not extract file name from source path".to_string(),
        }
    })?;

    // Get the vault name (without .db extension)
    let vault_name = file_name.trim_end_matches(VAULT_EXTENSION);

    // Check if vault already exists
    let target_path = get_vault_path(&app_handle, vault_name)?;
    if Path::new(&target_path).exists() {
        return Err(DatabaseError::VaultAlreadyExists {
            vault_name: vault_name.to_string(),
        });
    }

    // Copy the file to the vaults directory
    fs::copy(&source_path, &target_path).map_err(|e| DatabaseError::IoError {
        path: target_path.clone(),
        reason: format!("Failed to copy vault file: {e}"),
    })?;

    println!(
        "Vault '{}' successfully imported to '{}'",
        vault_name, target_path
    );

    Ok(target_path)
}

/// Moves a vault database file to trash (or deletes permanently if trash is unavailable)
#[tauri::command]
pub fn move_vault_to_trash(
    app_handle: AppHandle,
    vault_name: String,
) -> Result<String, DatabaseError> {
    // On Android, trash is not available, so delete permanently
    #[cfg(target_os = "android")]
    {
        println!(
            "Android platform detected, permanently deleting vault '{}'",
            vault_name
        );
        return delete_vault(app_handle, vault_name);
    }

    // On non-Android platforms, try to use trash
    #[cfg(not(target_os = "android"))]
    {
        let vault_path = get_vault_path(&app_handle, &vault_name)?;
        let vault_shm_path = format!("{vault_path}-shm");
        let vault_wal_path = format!("{vault_path}-wal");

        if !Path::new(&vault_path).exists() {
            return Err(DatabaseError::IoError {
                path: vault_path,
                reason: "Vault does not exist".to_string(),
            });
        }

        // Try to move to trash first (works on desktop systems)
        let moved_to_trash = trash::delete(&vault_path).is_ok();

        if moved_to_trash {
            // Also try to move auxiliary files to trash (ignore errors as they might not exist)
            let _ = trash::delete(&vault_shm_path);
            let _ = trash::delete(&vault_wal_path);

            Ok(format!("Vault '{vault_name}' successfully moved to trash"))
        } else {
            // Fallback: Permanent deletion if trash fails
            println!(
                "Trash not available, falling back to permanent deletion for vault '{vault_name}'"
            );
            delete_vault(app_handle, vault_name)
        }
    }
}

/// Deletes a vault database file permanently (bypasses trash)
#[tauri::command]
pub fn delete_vault(app_handle: AppHandle, vault_name: String) -> Result<String, DatabaseError> {
    let vault_path = get_vault_path(&app_handle, &vault_name)?;
    let vault_shm_path = format!("{vault_path}-shm");
    let vault_wal_path = format!("{vault_path}-wal");

    if !Path::new(&vault_path).exists() {
        return Err(DatabaseError::IoError {
            path: vault_path,
            reason: "Vault does not exist".to_string(),
        });
    }

    if Path::new(&vault_shm_path).exists() {
        fs::remove_file(&vault_shm_path).map_err(|e| DatabaseError::IoError {
            path: vault_shm_path.clone(),
            reason: format!("Failed to delete vault: {e}"),
        })?;
    }

    if Path::new(&vault_wal_path).exists() {
        fs::remove_file(&vault_wal_path).map_err(|e| DatabaseError::IoError {
            path: vault_wal_path.clone(),
            reason: format!("Failed to delete vault: {e}"),
        })?;
    }

    fs::remove_file(&vault_path).map_err(|e| DatabaseError::IoError {
        path: vault_path.clone(),
        reason: format!("Failed to delete vault: {e}"),
    })?;

    Ok(format!("Vault '{vault_name}' successfully deleted"))
}

#[tauri::command]
pub fn create_encrypted_database(
    app_handle: AppHandle,
    vault_name: String,
    key: String,
    vault_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, DatabaseError> {
    println!("Creating encrypted vault with name: {vault_name}");

    let vault_path = get_vault_path(&app_handle, &vault_name)?;
    println!("Resolved vault path: {vault_path}");

    // Prüfen, ob bereits eine Vault mit diesem Namen existiert
    if Path::new(&vault_path).exists() {
        return Err(DatabaseError::IoError {
            path: vault_path.clone(),
            reason: format!("A vault with the name '{vault_name}' already exists"),
        });
    }

    println!("Creating new empty encrypted database at: {}", &vault_path);

    // Step 1: Create empty encrypted database
    {
        let conn = Connection::open(&vault_path).map_err(|e| DatabaseError::ConnectionFailed {
            path: vault_path.clone(),
            reason: format!("Failed to create database file: {}", e),
        })?;

        // Set encryption key immediately
        conn.pragma_update(None, "key", &key)
            .map_err(|e| DatabaseError::PragmaError {
                pragma: "key".to_string(),
                reason: e.to_string(),
            })?;

        // Verify SQLCipher is active
        println!("Verifying SQLCipher encryption...");
        match conn.query_row("PRAGMA cipher_version;", [], |row| {
            let version: String = row.get(0)?;
            Ok(version)
        }) {
            Ok(version) => {
                println!("✅ SQLCipher is active! Version: {}", version);
            }
            Err(e) => {
                eprintln!("❌ ERROR: SQLCipher is NOT active!");
                eprintln!("PRAGMA cipher_version failed: {}", e);
                let _ = fs::remove_file(&vault_path);
                return Err(DatabaseError::DatabaseError {
                    reason: format!("SQLCipher verification failed: {}", e),
                });
            }
        }

        // Create a minimal table to initialize the database file
        // This forces SQLite to write the header and validates the encryption
        conn.execute("CREATE TABLE _init (id INTEGER PRIMARY KEY);", [])
            .map_err(|e| DatabaseError::ExecutionError {
                sql: "CREATE TABLE _init".to_string(),
                reason: e.to_string(),
                table: Some("_init".to_string()),
            })?;

        conn.execute("DROP TABLE _init;", [])
            .map_err(|e| DatabaseError::ExecutionError {
                sql: "DROP TABLE _init".to_string(),
                reason: e.to_string(),
                table: Some("_init".to_string()),
            })?;

        conn.close()
            .map_err(|(_, e)| DatabaseError::ConnectionFailed {
                path: vault_path.clone(),
                reason: format!("Failed to close database after initialization: {}", e),
            })?;
    }

    println!("[CREATE_DB] ✅ Empty encrypted database created successfully");

    // Step 2: Open the database and store connection in AppState (without full initialization)
    // We need the connection available for migrations, but can't initialize HLC yet
    // because haex_crdt_configs table doesn't exist until migrations run
    println!("[CREATE_DB] Step 2: Opening database connection for migrations...");
    let conn = core::open_and_init_db(&vault_path, &key, false)?;
    println!("[CREATE_DB] Database connection opened successfully");

    // Store connection in AppState
    println!("[CREATE_DB] Storing connection in AppState...");
    {
        let mut db_guard = state.db.0.lock().map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
        *db_guard = Some(conn);
    }
    println!("[CREATE_DB] Connection stored in AppState");

    // Step 3: Apply core migrations to build the schema
    println!("[CREATE_DB] Step 3: Applying core migrations...");
    let migrations_applied =
        crate::database::migrations::apply_core_migrations(app_handle.clone(), state.clone())?;

    println!(
        "[CREATE_DB] ✅ {} core migrations applied",
        migrations_applied
    );

    // Step 4: Now initialize HLC and triggers (tables exist after migrations)
    println!("[CREATE_DB] Step 4: Initializing HLC and CRDT triggers...");
    initialize_session_post_migration(&app_handle, &state)?;
    println!("[CREATE_DB] ✅ HLC and triggers initialized");

    // Step 5: Set vault_id - but only for NEW vaults (not for remote sync)
    // When connecting to a remote vault (vault_id provided), skip this step
    // because the vault_id setting will be pulled from the server
    println!("[CREATE_DB] Step 5: Setting vault_id...");
    if vault_id.is_some() {
        println!(
            "[CREATE_DB] Remote sync mode: Skipping vault_id insert (will be pulled from server)"
        );
    } else {
        // Generate new vault_id for newly created vaults
        let new_vault_id = uuid::Uuid::new_v4().to_string();
        println!("[CREATE_DB] Generating new vault_id: {}", new_vault_id);

        // Use HLC service to insert with proper CRDT timestamps
        let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
            reason: "Failed to lock HLC service".to_string(),
        })?;
        let row_id = uuid::Uuid::new_v4().to_string();
        let insert_sql = format!(
            "INSERT INTO {} (id, key, type, value) VALUES (?, 'vault_id', 'string', ?)",
            TABLE_VAULT_SETTINGS
        );
        with_connection(&state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;
            SqlExecutor::execute_internal_typed(
                &tx,
                &hlc_service,
                &insert_sql,
                rusqlite::params![row_id, new_vault_id],
            )?;
            tx.commit().map_err(DatabaseError::from)?;
            Ok(())
        })?;
        println!("[CREATE_DB] ✅ vault_id set successfully with CRDT timestamp");
    }

    println!("[CREATE_DB] ========== create_encrypted_database COMPLETE ==========");
    Ok(vault_path)
}

#[tauri::command]
pub fn open_encrypted_database(
    app_handle: AppHandle,
    vault_path: String,
    key: String,
    state: State<'_, AppState>,
) -> Result<String, DatabaseError> {
    println!("[OPEN_DB] open_encrypted_database called for: {vault_path}");

    // Check if a database connection already exists in AppState
    // This happens when create_encrypted_database was called before
    let already_open = {
        let db_guard = state.db.0.lock().map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
        db_guard.is_some()
    };

    if already_open {
        println!(
            "[OPEN_DB] Database connection already exists in AppState, skipping re-initialization"
        );
        return Ok(format!("Vault '{vault_path}' already open"));
    }

    println!("[OPEN_DB] No existing connection, initializing new session...");

    if !Path::new(&vault_path).exists() {
        return Err(DatabaseError::IoError {
            path: vault_path.to_string(),
            reason: format!("Vault '{vault_path}' does not exist"),
        });
    }

    initialize_session(&app_handle, &vault_path, &key, &state)?;

    println!("[OPEN_DB] ✅ Vault opened successfully");
    Ok(format!("Vault '{vault_path}' opened successfully"))
}

/// Initializes HLC and triggers AFTER migrations have been applied.
/// Used by create_encrypted_database where the connection is already in AppState.
fn initialize_session_post_migration(
    app_handle: &AppHandle,
    state: &State<'_, AppState>,
) -> Result<(), DatabaseError> {
    // Connection is already in AppState, we just need to initialize HLC and triggers
    with_connection(&state.db, |conn| {
        // 1. Ensure CRDT triggers are initialized
        let triggers_were_already_initialized = init::ensure_triggers_initialized(conn)?;

        // 2. Initialize the HLC service
        let hlc_service = HlcService::try_initialize(conn, app_handle).map_err(|e| {
            DatabaseError::ExecutionError {
                sql: "HLC Initialization".to_string(),
                reason: e.to_string(),
                table: Some(TABLE_CRDT_CONFIGS.to_string()),
            }
        })?;

        // 3. Store HLC service in AppState
        let mut hlc_guard = state.hlc.lock().map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
        *hlc_guard = hlc_service;
        drop(hlc_guard);

        // 4. Set triggers_initialized flag if needed
        if !triggers_were_already_initialized {
            eprintln!("INFO: Setting 'triggers_initialized' flag...");
            conn.execute(
                &format!(
                    "INSERT INTO {TABLE_VAULT_SETTINGS} (id, key, type, value) VALUES (?, ?, ?, ?)"
                ),
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    "triggers_initialized",
                    "system",
                    "1"
                ],
            )
            .map_err(DatabaseError::from)?;
        }

        Ok(())
    })
}

/// Opens the DB, initializes the HLC service, and stores both in the AppState.
fn initialize_session(
    app_handle: &AppHandle,
    path: &str,
    key: &str,
    state: &State<'_, AppState>,
) -> Result<(), DatabaseError> {
    // 1. Establish the raw database connection
    let mut conn = core::open_and_init_db(path, key, false)?;

    // 2. Ensure CRDT triggers are initialized
    let triggers_were_already_initialized = init::ensure_triggers_initialized(&mut conn)?;

    // 3. Initialize the HLC service
    let hlc_service = HlcService::try_initialize(&conn, app_handle).map_err(|e| {
        // We convert the HlcError into a DatabaseError
        DatabaseError::ExecutionError {
            sql: "HLC Initialization".to_string(),
            reason: e.to_string(),
            table: Some(TABLE_CRDT_CONFIGS.to_string()),
        }
    })?;

    // 4. Store everything in the global AppState
    let mut db_guard = state.db.0.lock().map_err(|e| DatabaseError::LockError {
        reason: e.to_string(),
    })?;
    // Wichtig: Wir brauchen den db_guard gleich nicht mehr,
    // da 'execute_with_crdt' 'with_connection' aufruft, was
    // 'state.db' selbst locken muss.
    // Wir müssen den Guard freigeben, *bevor* wir 'execute_with_crdt' rufen,
    // um einen Deadlock zu verhindern.
    // Aber wir müssen die 'conn' erst hineinbewegen.
    *db_guard = Some(conn);
    drop(db_guard);

    let mut hlc_guard = state.hlc.lock().map_err(|e| DatabaseError::LockError {
        reason: e.to_string(),
    })?;
    *hlc_guard = hlc_service;

    // WICHTIG: hlc_guard *nicht* freigeben, da 'execute_with_crdt'
    // eine Referenz auf die Guard erwartet.

    // 5. NEUER SCHRITT: Setze das Flag via CRDT, falls nötig
    if !triggers_were_already_initialized {
        eprintln!("INFO: Setting 'triggers_initialized' flag via CRDT...");

        let insert_sql = format!(
            "INSERT INTO {TABLE_VAULT_SETTINGS} (id, key, type, value) VALUES (?, ?, ?, ?)"
        );

        // execute_with_crdt erwartet Vec<JsonValue>, kein params!-Makro
        let params_vec: Vec<JsonValue> = vec![
            JsonValue::String(uuid::Uuid::new_v4().to_string()),
            JsonValue::String("triggers_initialized".to_string()),
            JsonValue::String("system".to_string()),
            JsonValue::String("1".to_string()),
        ];

        // Jetzt können wir 'execute_with_crdt' sicher aufrufen,
        // da der AppState initialisiert ist.
        execute_with_crdt(
            insert_sql, params_vec, &state.db,  // Das &DbConnection (der Mutex)
            &hlc_guard, // Die gehaltene MutexGuard
        )?;

        eprintln!("INFO: ✓ 'triggers_initialized' flag set.");
    }

    Ok(())
}

/// Cleans up old tombstones by hard-deleting rows with haex_tombstone = 1
/// that are older than the specified retention period.
///
/// This prevents unbounded table growth from soft-deleted entries.
#[tauri::command]
pub fn crdt_cleanup_tombstones(
    retention_days: u32,
    state: State<'_, AppState>,
) -> Result<crate::crdt::cleanup::CleanupResult, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        crate::crdt::cleanup::cleanup_tombstones(conn, retention_days).map_err(|e| {
            DatabaseError::ExecutionError {
                sql: "CRDT cleanup".to_string(),
                reason: e.to_string(),
                table: None,
            }
        })
    })
}

/// Gets statistics about CRDT tables (total entries, tombstoned entries, etc.)
#[tauri::command]
pub fn crdt_get_stats(
    state: State<'_, AppState>,
) -> Result<crate::crdt::cleanup::CrdtStats, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        crate::crdt::cleanup::get_crdt_stats(conn).map_err(|e| DatabaseError::ExecutionError {
            sql: "CRDT stats".to_string(),
            reason: e.to_string(),
            table: None,
        })
    })
}

/// Runs SQLite VACUUM command to reclaim disk space
#[tauri::command]
pub fn database_vacuum(state: State<'_, AppState>) -> Result<String, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        conn.execute("VACUUM", [])
            .map_err(|e| DatabaseError::ExecutionError {
                sql: "VACUUM".to_string(),
                reason: e.to_string(),
                table: None,
            })?;
        Ok("Database vacuumed successfully".to_string())
    })
}

/// Changes the vault password using SQLCipher's rekey functionality.
/// This re-encrypts the entire database with the new password.
///
/// IMPORTANT: The old password must be correct (database must already be open).
/// After this operation, the database will be encrypted with the new password.
#[tauri::command]
pub fn change_vault_password(
    new_password: String,
    state: State<'_, AppState>,
) -> Result<String, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        // Use PRAGMA rekey to change the encryption key
        // This re-encrypts the entire database with the new key
        conn.pragma_update(None, "rekey", &new_password)
            .map_err(|e| DatabaseError::PragmaError {
                pragma: "rekey".to_string(),
                reason: e.to_string(),
            })?;

        println!("✅ Vault password changed successfully via SQLCipher rekey");
        Ok("Vault password changed successfully".to_string())
    })
}

// src-tauri/src/database/mod.rs

pub mod connection_context;
pub mod constants;
pub mod core;
pub mod error;
pub mod generated;
pub mod init;
pub mod migrations;
pub mod row;
pub mod stats;
pub mod vault_lock;

use crate::crdt::hlc::HlcService;
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::event_names::EVENT_CRDT_DIRTY_TABLES_CHANGED;
use crate::extension::database::executor::SqlExecutor;
use crate::table_names::{COL_CRDT_CONFIGS_KEY, COL_CRDT_CONFIGS_TYPE, COL_CRDT_CONFIGS_VALUE, TABLE_CRDT_CONFIGS, TABLE_VAULT_SETTINGS};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use constants::vault_settings_key;
use crate::AppState;
use ed25519_dalek::SigningKey;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
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
    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

    Ok(result)
}

/// DEPRECATED: Use sql_with_crdt instead
/// This command is kept for backwards compatibility
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
    let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

    Ok(result)
}

/// Unified SQL command with CRDT support
///
/// This command automatically detects the SQL statement type using AST parsing:
/// - SELECT: Executes with tombstone filtering (select_with_crdt)
/// - INSERT/UPDATE/DELETE: Executes with CRDT timestamps (execute_with_crdt)
///   - If RETURNING clause is present, returns the result rows
///   - Otherwise returns empty array
///
/// This replaces the need for separate sql_select_with_crdt, sql_execute_with_crdt,
/// and sql_query_with_crdt commands in the frontend.
#[tauri::command]
pub fn sql_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    use sqlparser::ast::Statement;

    // Parse the SQL statement using AST (no string matching!)
    let statement = core::parse_single_statement(&sql)?;

    match statement {
        // SELECT statements: use select_with_crdt (adds tombstone filter)
        Statement::Query(_) => core::select_with_crdt(sql, params, &state.db),
        // INSERT/UPDATE/DELETE: use execute_with_crdt (handles RETURNING via AST)
        Statement::Insert(_) | Statement::Update { .. } | Statement::Delete(_) => {
            let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
                reason: "Failed to lock HLC service".to_string(),
            })?;

            let result = core::execute_with_crdt(sql, params, &state.db, &hlc_service)?;

            // Emit event to notify frontend that dirty tables may have changed
            let _ = app_handle.emit(EVENT_CRDT_DIRTY_TABLES_CHANGED, ());

            Ok(result)
        }
        // Other statements (CREATE TABLE, etc.) - execute without CRDT
        _ => core::execute(sql, params, &state.db),
    }
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
pub fn import_vault(app_handle: AppHandle, source_path: String, vault_name: Option<String>) -> Result<String, DatabaseError> {
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

    // Use provided vault_name or derive from file name
    let vault_name = match vault_name {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => {
            let file_name = source.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                DatabaseError::ValidationError {
                    reason: "Could not extract file name from source path".to_string(),
                }
            })?;
            file_name.trim_end_matches(VAULT_EXTENSION).to_string()
        }
    };

    // Check if vault already exists
    let target_path = get_vault_path(&app_handle, &vault_name)?;
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
            // Vault file already gone — not an error, just clean up references
            return Ok(format!("Vault '{vault_name}' already removed"));
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
        // Vault file already gone — not an error, just clean up references
        return Ok(format!("Vault '{vault_name}' already removed"));
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

// ---------------------------------------------------------------------------
// Default identity bootstrap
//
// Every vault needs at least one "own" identity (a row in haex_identities with
// a non-null private_key) before anything identity-backed can happen:
// haex_spaces.owner_identity_id is NOT NULL, UCAN signing needs a private key,
// and the JS side looks one up on vault mount.
//
// The UI flow (vault.vue onMounted → ensureDefaultIdentityAsync) handles this
// for humans, but direct-Tauri paths (e.g. E2E tests that invoke
// create_encrypted_database / open_encrypted_database without navigating)
// would otherwise leave the vault without an identity. Seeding it here
// guarantees that every freshly-opened vault is immediately usable.
//
// The key format mirrors the JS side so JS can transparently load and sign
// with keys created here:
//   - private_key: Base64-encoded 48-byte Ed25519 PKCS8 (16-byte prefix + 32-byte seed)
//   - did: `did:key:z` + base58btc(0xed01 || raw-public-key)
// ---------------------------------------------------------------------------

// Ed25519 multicodec tag used in did:key format.
const DEFAULT_IDENTITY_ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

// PKCS8 ASN.1 wrapper used by WebCrypto's exportKey('pkcs8') for Ed25519.
// SEQUENCE(46) → INTEGER(0) → AlgorithmId(OID 1.3.101.112) → OCTET STRING(34 → OCTET STRING(32))
const DEFAULT_IDENTITY_ED25519_PKCS8_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// Default English name for auto-seeded identities. JS's
/// `ensureDefaultIdentityAsync` recognises this value and will re-localise /
/// generate an avatar on first UI mount, so we don't need locale awareness
/// in Rust.
const DEFAULT_IDENTITY_NAME: &str = "My Identity";

fn generate_default_identity_material() -> (String, String) {
    let mut seed = [0u8; 32];
    rand::fill(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let public_key = signing_key.verifying_key();

    // did:key:z<base58btc(multicodec || pubkey)>
    let mut did_bytes = Vec::with_capacity(34);
    did_bytes.extend_from_slice(&DEFAULT_IDENTITY_ED25519_MULTICODEC);
    did_bytes.extend_from_slice(public_key.as_bytes());
    let did = format!("did:key:z{}", bs58::encode(did_bytes).into_string());

    // PKCS8(prefix || seed), Base64 (matches crypto.subtle.exportKey('pkcs8')).
    let mut pkcs8 = Vec::with_capacity(48);
    pkcs8.extend_from_slice(&DEFAULT_IDENTITY_ED25519_PKCS8_PREFIX);
    pkcs8.extend_from_slice(&seed);
    let private_key_b64 = BASE64.encode(&pkcs8);

    (did, private_key_b64)
}

/// Ensures the currently open vault has at least one own identity. Idempotent:
/// becomes a no-op when a row with private_key IS NOT NULL already exists.
fn ensure_default_identity(state: &State<'_, AppState>) -> Result<(), DatabaseError> {
    // CRDT-aware existence check: select_with_crdt strips tombstoned rows,
    // so a previously-deleted default identity doesn't suppress re-seeding.
    let existing = core::select_with_crdt(
        "SELECT id FROM haex_identities WHERE private_key IS NOT NULL LIMIT 1".to_string(),
        vec![],
        &state.db,
    )?;
    if !existing.is_empty() {
        return Ok(());
    }

    let (did, private_key_b64) = generate_default_identity_material();
    let id = uuid::Uuid::new_v4().to_string();

    let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
        reason: "Failed to lock HLC service".to_string(),
    })?;

    core::execute_with_crdt(
        "INSERT INTO haex_identities (id, did, name, source, private_key) VALUES (?1, ?2, ?3, 'contact', ?4)".to_string(),
        vec![
            JsonValue::String(id),
            JsonValue::String(did.clone()),
            JsonValue::String(DEFAULT_IDENTITY_NAME.to_string()),
            JsonValue::String(private_key_b64),
        ],
        &state.db,
        &hlc_service,
    )?;

    println!("[IDENTITY] ✅ default identity seeded ({})", &did[..30.min(did.len())]);
    Ok(())
}

#[tauri::command]
pub fn create_encrypted_database(
    app_handle: AppHandle,
    vault_name: String,
    key: String,
    space_id: Option<String>,
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

    // Refuse to mount a new vault while another is still live in this
    // process. Writing to `state.vault_lock` blindly would drop the prior
    // `VaultLock` (releasing its flock) while its SQLite connection is
    // still open in `state.db` — the first vault would then be exposed to
    // concurrent writers from other instances.
    reject_if_vault_already_mounted(&state)?;

    // Acquire the per-vault lock up front. A freshly-created vault can't
    // collide with another instance by definition, but grabbing the lock
    // here keeps the create-then-open-then-close lifecycle symmetric with
    // `open_encrypted_database` — and any surprising race (e.g. two
    // parallel `create` calls for the exact same filename) is cleanly
    // rejected here instead of corrupting the half-written DB.
    acquire_vault_lock_or_error(&vault_path, &state)?;

    // Wrap the remaining steps in a closure so any `?`-propagated error
    // runs a full teardown on the way out. Releasing only the lock would
    // leave a half-initialized session (connection, HLC, ctx) in AppState
    // which breaks subsequent `open_encrypted_database` retries.
    let outcome: Result<String, DatabaseError> = (|| {
        create_encrypted_database_inner(&app_handle, &vault_path, &key, space_id, &state)
    })();

    if outcome.is_err() {
        let _ = close_database(state.clone());
    }

    outcome
}

fn create_encrypted_database_inner(
    app_handle: &AppHandle,
    vault_path: &str,
    key: &str,
    space_id: Option<String>,
    state: &State<'_, AppState>,
) -> Result<String, DatabaseError> {
    let vault_path = vault_path.to_string();
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
    let hlc_for_conn = state
        .hlc
        .lock()
        .map_err(|e| DatabaseError::LockError { reason: e.to_string() })?
        .clone();
    let ctx_for_conn = state
        .connection_context
        .lock()
        .map_err(|e| DatabaseError::LockError { reason: e.to_string() })?
        .clone();
    let conn = core::open_and_init_db(&vault_path, &key, false, hlc_for_conn, ctx_for_conn)?;
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
    initialize_session_post_migration(app_handle, state)?;
    println!("[CREATE_DB] ✅ HLC and triggers initialized");

    // Step 5: Set space_id
    // For remote sync: use the provided space_id
    // For new vaults: generate a new UUID
    println!("[CREATE_DB] Step 5: Setting space_id...");
    {
        let effective_space_id = match &space_id {
            Some(id) => {
                println!("[CREATE_DB] Using provided space_id: {}", id);
                id.clone()
            }
            None => {
                let new_id = uuid::Uuid::new_v4().to_string();
                println!("[CREATE_DB] Generating new space_id: {}", new_id);
                new_id
            }
        };

        let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
            reason: "Failed to lock HLC service".to_string(),
        })?;
        let row_id = uuid::Uuid::new_v4().to_string();
        let insert_sql = format!(
            "INSERT INTO {} (id, key, value) VALUES (?, '{}', ?)",
            TABLE_VAULT_SETTINGS,
            vault_settings_key::SPACE_ID,
        );
        with_connection(&state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;
            SqlExecutor::execute_internal_typed(
                &tx,
                &hlc_service,
                &insert_sql,
                rusqlite::params![row_id, effective_space_id],
            )?;
            tx.commit().map_err(DatabaseError::from)?;
            Ok(())
        })?;
        println!("[CREATE_DB] ✅ space_id set successfully with CRDT timestamp");
    }

    // Step 6: Generate device_key_secret (32 random bytes for encrypting the Ed25519 device key file)
    println!("[CREATE_DB] Step 6: Generating device_key_secret...");
    {
        let mut secret_bytes = [0u8; 32];
        rand::fill(&mut secret_bytes);
        let secret_hex = hex::encode(secret_bytes);

        let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
            reason: "Failed to lock HLC service".to_string(),
        })?;
        let row_id = uuid::Uuid::new_v4().to_string();
        let insert_sql = format!(
            "INSERT INTO {} (id, key, value) VALUES (?, '{}', ?)",
            TABLE_VAULT_SETTINGS,
            vault_settings_key::DEVICE_KEY_SECRET,
        );
        with_connection(&state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;
            SqlExecutor::execute_internal_typed(
                &tx,
                &hlc_service,
                &insert_sql,
                rusqlite::params![row_id, secret_hex],
            )?;
            tx.commit().map_err(DatabaseError::from)?;
            Ok(())
        })?;
        println!("[CREATE_DB] ✅ device_key_secret generated and stored");
    }

    // Step 7: Seed the default own identity so the vault is immediately usable
    // (haex_spaces.owner_identity_id is NOT NULL; UCAN signing needs this key).
    println!("[CREATE_DB] Step 7: Seeding default identity...");
    ensure_default_identity(state)?;
    println!("[CREATE_DB] ✅ default identity ensured");

    println!("[CREATE_DB] ========== create_encrypted_database COMPLETE ==========");
    Ok(vault_path)
}

/// Closes the current database connection and resets related state.
/// This must be called before opening a different vault.
#[tauri::command]
pub fn close_database(state: State<'_, AppState>) -> Result<(), DatabaseError> {
    println!("[CLOSE_DB] Closing database connection...");

    // 1. Close the database connection
    {
        let mut db_guard = state.db.0.lock().map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;

        if let Some(conn) = db_guard.take() {
            // Close the connection explicitly
            if let Err((_, e)) = conn.close() {
                eprintln!("[CLOSE_DB] Warning: Failed to close database cleanly: {}", e);
            }
            println!("[CLOSE_DB] Database connection closed");
        } else {
            println!("[CLOSE_DB] No database connection to close");
        }
    }

    // 2. Reset HLC service
    {
        let mut hlc_guard = state.hlc.lock().map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
        *hlc_guard = HlcService::default();
        println!("[CLOSE_DB] HLC service reset");
    }

    // 3. Reset the per-session connection context so any leftover tx-HLC slot
    //    from the previous vault cannot leak into the next one.
    {
        let mut ctx_guard = state
            .connection_context
            .lock()
            .map_err(|e| DatabaseError::LockError { reason: e.to_string() })?;
        *ctx_guard = connection_context::ConnectionContext::new();
        println!("[CLOSE_DB] ConnectionContext reset");
    }

    // 3. Clear extension manager caches
    {
        if let Ok(mut available_exts) = state.extension_manager.available_extensions.lock() {
            available_exts.clear();
            println!("[CLOSE_DB] Available extensions cache cleared");
        }
        if let Ok(mut perm_cache) = state.extension_manager.permission_cache.lock() {
            perm_cache.clear();
            println!("[CLOSE_DB] Permission cache cleared");
        }
        if let Ok(mut missing) = state.extension_manager.missing_extensions.lock() {
            missing.clear();
            println!("[CLOSE_DB] Missing extensions list cleared");
        }
    }

    // 4. Release the per-vault advisory lock so another instance (or a
    //    subsequent `open_encrypted_database` in this process) can mount
    //    the same vault again. Dropping the `VaultLock` releases flock.
    release_vault_lock(&state);
    println!("[CLOSE_DB] Vault file lock released");

    println!("[CLOSE_DB] ✅ Database closed and state reset");
    Ok(())
}

/// Try to grab the exclusive per-vault advisory lock and stash it in
/// AppState. Returns `VaultAlreadyOpenElsewhere` when another instance has
/// this exact path locked.
fn acquire_vault_lock_or_error(
    vault_path: &str,
    state: &State<'_, AppState>,
) -> Result<(), DatabaseError> {
    let lock = vault_lock::VaultLock::try_acquire(Path::new(vault_path)).map_err(|e| match e {
        vault_lock::VaultLockError::AlreadyHeld { path, source } => {
            DatabaseError::VaultAlreadyOpenElsewhere {
                path,
                reason: source.to_string(),
            }
        }
        vault_lock::VaultLockError::Io { path, source } => DatabaseError::IoError {
            path,
            reason: format!("vault lock file: {source}"),
        },
    })?;

    let mut guard = state.vault_lock.lock().map_err(|e| DatabaseError::LockError {
        reason: e.to_string(),
    })?;
    *guard = Some(lock);
    Ok(())
}

/// Reject mount attempts when this process already has a vault open.
/// Prevents dropping the live `VaultLock` (and releasing its flock) while
/// the corresponding SQLite connection is still stored in `state.db`.
fn reject_if_vault_already_mounted(state: &State<'_, AppState>) -> Result<(), DatabaseError> {
    let has_connection = state
        .db
        .0
        .lock()
        .map_err(|e| DatabaseError::LockError { reason: e.to_string() })?
        .is_some();
    let has_lock = state
        .vault_lock
        .lock()
        .map_err(|e| DatabaseError::LockError { reason: e.to_string() })?
        .is_some();
    if has_connection || has_lock {
        return Err(DatabaseError::DatabaseError {
            reason: "Another vault is still mounted in this process; close it first."
                .to_string(),
        });
    }
    Ok(())
}

/// Drop any currently-held vault lock, releasing the OS advisory lock.
/// Best-effort: a poisoned mutex here would only block future opens, which
/// is preferable to panicking in shutdown / error-recovery paths.
fn release_vault_lock(state: &State<'_, AppState>) {
    match state.vault_lock.lock() {
        Ok(mut guard) => {
            *guard = None;
        }
        Err(e) => {
            eprintln!("[CLOSE_DB] Warning: vault_lock mutex poisoned, skipping release: {e}");
        }
    }
}

#[tauri::command]
pub fn open_encrypted_database(
    app_handle: AppHandle,
    vault_path: String,
    key: String,
    state: State<'_, AppState>,
) -> Result<String, DatabaseError> {
    println!("[OPEN_DB] open_encrypted_database called for: {vault_path}");

    // Check whether a vault is already mounted in this process, and whether
    // it's the one the caller is asking for. The create → open chain leaves
    // the connection live on purpose (idempotent success); but if a
    // *different* vault is mounted, returning "already open" here would
    // silently hand the caller the wrong vault's data.
    let mounted_path: Option<PathBuf> = {
        let lock_guard = state.vault_lock.lock().map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
        lock_guard.as_ref().map(|lock| lock.vault_path().to_path_buf())
    };

    if let Some(existing) = mounted_path {
        if existing == Path::new(&vault_path) {
            println!(
                "[OPEN_DB] Vault '{vault_path}' already mounted (create→open flow); returning idempotent success"
            );
            return Ok(format!("Vault '{vault_path}' already open"));
        }
        return Err(DatabaseError::DatabaseError {
            reason: format!(
                "Cannot open '{vault_path}': a different vault ('{}') is already mounted in this process. Close it before opening another.",
                existing.display()
            ),
        });
    }

    println!("[OPEN_DB] No existing connection, initializing new session...");

    if !Path::new(&vault_path).exists() {
        return Err(DatabaseError::IoError {
            path: vault_path.to_string(),
            reason: format!("Vault '{vault_path}' does not exist"),
        });
    }

    // Acquire the per-vault exclusive lock BEFORE touching SQLite. If another
    // instance holds it, bail out with a dedicated error variant the frontend
    // recognises — opening the DB anyway would race the other instance's WAL
    // and HLC state.
    acquire_vault_lock_or_error(&vault_path, &state)?;

    // Wrap the post-lock steps so any failure (session init, migrations,
    // identity seeding) unwinds through one teardown path. A stuck lock OR a
    // half-initialized session (connection mounted but migrations failed)
    // would both break subsequent reopens — `close_database` handles both.
    let outcome: Result<(), DatabaseError> = (|| {
        initialize_session(&app_handle, &vault_path, &key, &state)?;
        println!("[OPEN_DB] Checking for pending migrations...");
        crate::database::migrations::apply_core_migrations(app_handle.clone(), state.clone())?;
        // Backfill a default own identity for vaults that predate the
        // seeding step in create_encrypted_database (idempotent — no-op
        // when one already exists).
        ensure_default_identity(&state)?;
        Ok(())
    })();

    if let Err(err) = outcome {
        let _ = close_database(state.clone());
        return Err(err);
    }

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

        // 2. Initialize the HLC service *in place*. The connection already holds
        //    a clone of this HlcService inside the `current_hlc()` UDF closure,
        //    so we must mutate the existing instance rather than swapping it out
        //    — otherwise the UDF would keep looking at an uninitialized service.
        let hlc_guard = state.hlc.lock().map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
        hlc_guard
            .initialize_in_place(conn, app_handle)
            .map_err(|e| DatabaseError::ExecutionError {
                sql: "HLC Initialization".to_string(),
                reason: e.to_string(),
                table: Some(TABLE_CRDT_CONFIGS.to_string()),
            })?;
        drop(hlc_guard);

        // 4. Set triggers_initialized flag if needed (in haex_crdt_configs, local-only, not synced)
        if !triggers_were_already_initialized {
            eprintln!("INFO: Setting 'triggers_initialized' flag...");
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {TABLE_CRDT_CONFIGS} ({COL_CRDT_CONFIGS_KEY}, {COL_CRDT_CONFIGS_TYPE}, {COL_CRDT_CONFIGS_VALUE}) VALUES (?, ?, ?)"
                ),
                rusqlite::params![
                    vault_settings_key::TRIGGERS_INITIALIZED,
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
    // 1. Establish the raw database connection. We pass clones of the AppState
    //    HlcService and ConnectionContext so the `current_hlc()` UDF and the
    //    commit/rollback hooks stay in sync with the rest of the session.
    let hlc_for_conn = state
        .hlc
        .lock()
        .map_err(|e| DatabaseError::LockError { reason: e.to_string() })?
        .clone();
    let ctx_for_conn = state
        .connection_context
        .lock()
        .map_err(|e| DatabaseError::LockError { reason: e.to_string() })?
        .clone();
    let mut conn = core::open_and_init_db(path, key, false, hlc_for_conn, ctx_for_conn)?;

    // 2. Ensure CRDT triggers are initialized
    let _triggers_were_already_initialized = init::ensure_triggers_initialized(&mut conn)?;

    // 3. Initialize the HLC service *in place* on the AppState instance — the
    //    connection already holds a clone inside the `current_hlc()` UDF.
    {
        let hlc_guard = state.hlc.lock().map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
        hlc_guard
            .initialize_in_place(&conn, app_handle)
            .map_err(|e| DatabaseError::ExecutionError {
                sql: "HLC Initialization".to_string(),
                reason: e.to_string(),
                table: Some(TABLE_CRDT_CONFIGS.to_string()),
            })?;
    }

    // 4. Store the connection in the global AppState.
    let mut db_guard = state.db.0.lock().map_err(|e| DatabaseError::LockError {
        reason: e.to_string(),
    })?;
    *db_guard = Some(conn);
    drop(db_guard);

    Ok(())
}

/// Cleans up the delete-log by hard-deleting entries older than the retention
/// period. `retention_days == 0` clears the full delete-log.
#[tauri::command]
pub fn crdt_cleanup_deleted_rows(
    retention_days: u32,
    state: State<'_, AppState>,
) -> Result<crate::crdt::cleanup::CleanupResult, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        crate::crdt::cleanup::cleanup_deleted_rows(conn, retention_days).map_err(|e| {
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
///
/// NOTE: SQLCipher's rekey does NOT work in WAL mode. We must:
/// 1. Checkpoint and switch to DELETE journal mode
/// 2. Perform the rekey
/// 3. Switch back to WAL mode
#[tauri::command]
pub fn change_vault_password(
    new_password: String,
    state: State<'_, AppState>,
) -> Result<String, DatabaseError> {
    core::with_connection(&state.db, |conn| {
        println!("[REKEY] Starting vault password change...");

        // Step 1: Checkpoint the WAL file to ensure all data is in the main database
        println!("[REKEY] Checkpointing WAL file (TRUNCATE mode)...");
        conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")
            .map_err(|e| DatabaseError::PragmaError {
                pragma: "wal_checkpoint".to_string(),
                reason: e.to_string(),
            })?;

        // Step 2: Switch from WAL to DELETE journal mode
        // This is required because rekey does not work properly in WAL mode
        println!("[REKEY] Switching to DELETE journal mode...");
        let _: String = conn
            .pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))
            .map_err(|e| DatabaseError::PragmaError {
                pragma: "journal_mode=DELETE".to_string(),
                reason: e.to_string(),
            })?;

        // Step 3: Use PRAGMA rekey to change the encryption key
        // This re-encrypts the entire database with the new key
        println!("[REKEY] Executing rekey with new password...");
        conn.pragma_update(None, "rekey", &new_password)
            .map_err(|e| DatabaseError::PragmaError {
                pragma: "rekey".to_string(),
                reason: e.to_string(),
            })?;

        // Step 4: Switch back to WAL mode for better performance
        println!("[REKEY] Switching back to WAL journal mode...");
        let _: String = conn
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
            .map_err(|e| DatabaseError::PragmaError {
                pragma: "journal_mode=WAL".to_string(),
                reason: e.to_string(),
            })?;

        println!("✅ Vault password changed successfully via SQLCipher rekey");
        Ok("Vault password changed successfully".to_string())
    })
}

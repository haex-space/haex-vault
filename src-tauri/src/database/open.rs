use super::create::{acquire_vault_lock_or_error, close_database, open_critical_sink};
use super::identity_default::ensure_default_identity;
use super::*;

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::table_names::{
    COL_CRDT_CONFIGS_KEY, COL_CRDT_CONFIGS_TYPE, COL_CRDT_CONFIGS_VALUE, TABLE_CRDT_CONFIGS,
};
use crate::AppState;
use constants::vault_settings_key;
use std::path::Path;
use tauri::{AppHandle, State};

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
    //
    // Use `VaultLock::matches` so different spellings (relative path,
    // symlink alias, `./` prefix) of the same DB resolve to the same
    // identity — without that normalization the create→open flow could
    // misclassify itself as a cross-vault collision.
    let already_mounted = {
        let lock_guard = state
            .vault_lock
            .lock()
            .map_err(|e| DatabaseError::LockError {
                reason: e.to_string(),
            })?;
        lock_guard.as_ref().map(|lock| {
            (
                lock.matches(Path::new(&vault_path)),
                lock.vault_path().to_path_buf(),
            )
        })
    };

    if let Some((matches, existing)) = already_mounted {
        if matches {
            println!(
                "[OPEN_DB] Vault '{vault_path}' already mounted (create→open flow); returning idempotent success"
            );
            return Ok(format!("Vault '{vault_path}' already open"));
        }
        return Err(DatabaseError::VaultAlreadyMountedInProcess {
            existing_path: existing.display().to_string(),
            requested_path: vault_path.clone(),
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
        // Open the critical-notification sink right after migrations so
        // `haex_critical_notifications_no_sync` exists. See the
        // symmetric step in `create_encrypted_database_inner` for the
        // rationale.
        open_critical_sink(&vault_path, &key, &state)?;
        println!("[OPEN_DB] ✅ Critical-notification sink opened");
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
pub(super) fn initialize_session_post_migration(
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
        let hlc_guard = state.lock_or_fail(
            &state.hlc,
            crate::critical::CriticalFailureCode::HlcMutexPoisoned,
            "database::initialize_session_post_migration",
            serde_json::json!({}),
        )?;
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
        .map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?
        .clone();
    let ctx_for_conn = state
        .connection_context
        .lock()
        .map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?
        .clone();
    let mut conn = core::open_and_init_db(path, key, false, hlc_for_conn, ctx_for_conn)?;

    // 2. Ensure CRDT triggers are initialized
    let _triggers_were_already_initialized = init::ensure_triggers_initialized(&mut conn)?;

    // 3. Initialize the HLC service *in place* on the AppState instance — the
    //    connection already holds a clone inside the `current_hlc()` UDF.
    {
        let hlc_guard = state.lock_or_fail(
            &state.hlc,
            crate::critical::CriticalFailureCode::HlcMutexPoisoned,
            "database::initialize_session",
            serde_json::json!({}),
        )?;
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

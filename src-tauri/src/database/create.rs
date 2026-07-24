use super::identity_default::ensure_default_identity;
use super::open::initialize_session_post_migration;
use super::*;

use crate::crdt::hlc::HlcService;
use crate::database::error::DatabaseError;
use crate::AppState;
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, State};

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
    reject_if_vault_already_mounted(&state, &vault_path)?;

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
    let outcome: Result<String, DatabaseError> =
        (|| create_encrypted_database_inner(&app_handle, &vault_path, &key, space_id, &state))();

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

    // Step 3b: Open the critical-notification sink as soon as
    // `haex_critical_notifications_no_sync` exists (i.e. after migrations
    // ran). The sink has its own connection to the same SQLCipher file
    // so subsequent `state.lock_or_fail` calls — including the ones in
    // the seed-data steps below — can surface poison conditions to the
    // user via the banner instead of dying silently.
    open_critical_sink(&vault_path, key, state)?;
    println!("[CREATE_DB] ✅ Critical-notification sink opened");

    // Step 4: Now initialize HLC and triggers (tables exist after migrations)
    println!("[CREATE_DB] Step 4: Initializing HLC and CRDT triggers...");
    initialize_session_post_migration(app_handle, state)?;
    println!("[CREATE_DB] ✅ HLC and triggers initialized");

    // Step 5: Seed the built-in `__core__` extension row so peer/extension
    // tables that reference it via FK (haex_shared_space_sync, etc.) have a
    // valid parent on a freshly-created vault. `haex_extensions` is CRDT-synced,
    // so go through execute_with_crdt to attach HLC + column timestamps.
    println!("[CREATE_DB] Step 5: Seeding __core__ extension row...");
    {
        let hlc_service = state.lock_or_fail(
            &state.hlc,
            crate::critical::CriticalFailureCode::HlcMutexPoisoned,
            "database::create_encrypted_database_inner::seed_core",
            serde_json::json!({}),
        )?;
        core::execute_with_crdt(
            "INSERT OR IGNORE INTO haex_extensions \
             (id, public_key, name, version, signature, enabled, single_instance, display_mode, description) \
             VALUES ('__core__', '__core__', 'core', '0.0.0', '', 1, 0, 'auto', \
                     'haex-vault built-in core feature target')"
                .to_string(),
            vec![],
            &state.db,
            &hlc_service,
        )?;
    }
    println!("[CREATE_DB] ✅ __core__ extension seeded");

    // Step 6: Seed the default own identity so the vault is immediately usable
    // (haex_spaces.owner_identity_id is NOT NULL; UCAN signing needs this key).
    println!("[CREATE_DB] Step 6: Seeding default identity...");
    ensure_default_identity(state)?;
    println!("[CREATE_DB] ✅ default identity ensured");

    // Step 7: Warm the column-signature signing-key cache. A brand-new
    // vault has no owned space memberships yet (returns 0), but the
    // symmetric call in `open_encrypted_database` requires the same
    // AppState field — keeping both callsites parallel avoids a class of
    // "cache was empty because create skipped the warm-up" bugs.
    if let Err(e) =
        super::open::populate_column_sig_key_cache(&state.column_sig_key_cache, &state.db)
    {
        eprintln!("[CREATE_DB] warn: SpaceKeyCache::populate_all failed: {e:?}");
    }

    // `space_id` is intentionally NOT seeded anymore. The legacy vault-UUID
    // setting was conceptually a device identity proxy; the device-identity
    // refactor moves that into the `haex_devices` table + <app_data>/device_id
    // file. The `space_id` parameter is kept on the signature for backwards
    // compatibility with the frontend invoke shape but is unused.
    let _ = space_id;

    println!("[CREATE_DB] ========== create_encrypted_database COMPLETE ==========");
    Ok(vault_path)
}

/// Closes the current database connection and resets related state.
/// This must be called before opening a different vault.
#[tauri::command]
pub fn close_database(state: State<'_, AppState>) -> Result<(), DatabaseError> {
    println!("[CLOSE_DB] Closing database connection...");

    // Stop vault-scoped background tasks BEFORE taking the connection:
    // sync loops clone `state.db.0` and would otherwise keep running with
    // a stale None — or, after the next vault opens, write through that
    // same Arc into the new vault. `peer_storage` is process-scoped (the
    // device's QUIC identity), so it stays up.
    tauri::async_runtime::block_on(async {
        // Drain sync-loop handles under the lock, then await outside. A
        // sync loop that is currently auto-disabling itself re-enters the
        // same mutex (`SyncManager::deregister`); awaiting its JoinHandle
        // while still holding `sync_manager.lock()` would deadlock here.
        let drained = {
            let mut manager = state.sync_manager.lock().await;
            manager.take_stop_all()
        };
        for (rule_id, handle) in drained {
            if let Err(join_err) = handle.await {
                eprintln!(
                    "[CLOSE_DB] sync-loop task for rule {rule_id} terminated abnormally: {join_err}"
                );
            }
        }
        // Same drain-then-await-outside-the-lock pattern as `sync_manager`
        // above, applied to background mail-poll watches.
        let drained_mail_polls = {
            let mut manager = state.mail_poll_manager.lock().await;
            manager.take_stop_all()
        };
        for (key, handle) in drained_mail_polls {
            if let Err(join_err) = handle.await {
                eprintln!("[CLOSE_DB] mail-poll task for {key} terminated abnormally: {join_err}");
            }
        }
        for (_, handle) in state.local_sync_loops.lock().await.drain() {
            handle.stop();
        }
        for (_, handle) in state.owner_sync_loops.lock().await.drain() {
            handle.stop();
        }
        state.leader_state.write().await.clear();
        for (_, (cancel, _)) in state.transfer_tokens.lock().await.drain() {
            cancel.cancel();
        }
    });
    println!("[CLOSE_DB] Runtime state cleared (sync loops, leaders, transfers)");

    // 1. Drop the critical-notification sink FIRST — its rusqlite
    //    connection is held independently of `state.db`, so closing it
    //    must not depend on the main DB mutex being healthy. Doing this
    //    before the db.lock() at step 2 is the whole point of the
    //    "separate connection" design: if `state.db.0` is poisoned (the
    //    very scenario the sink exists to surface), the ?-propagation at
    //    step 2 would otherwise return Err and leak the sink for the
    //    process lifetime.
    //
    //    `unwrap_or_else(into_inner)` is correct here because this IS
    //    the last layer of defense — there's no further mechanism to
    //    surface a poisoned sink-slot mutex.
    {
        let mut sink_guard = state
            .critical_sink
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if sink_guard.take().is_some() {
            println!("[CLOSE_DB] Critical-notification sink dropped");
        }
    }

    // 1b. Drop the log sink — same rationale as the critical sink.
    {
        let mut sink_guard = state.log_sink.lock().unwrap_or_else(|p| p.into_inner());
        if sink_guard.take().is_some() {
            println!("[CLOSE_DB] Log sink dropped");
        }
    }

    // 2. Close the main database connection.
    {
        let mut db_guard = state.db.0.lock().map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;

        if let Some(conn) = db_guard.take() {
            // Close the connection explicitly
            if let Err((_, e)) = conn.close() {
                eprintln!(
                    "[CLOSE_DB] Warning: Failed to close database cleanly: {}",
                    e
                );
            }
            println!("[CLOSE_DB] Database connection closed");
        } else {
            println!("[CLOSE_DB] No database connection to close");
        }
    }

    // 2. Reset HLC service
    {
        let mut hlc_guard = state.lock_or_fail(
            &state.hlc,
            crate::critical::CriticalFailureCode::HlcMutexPoisoned,
            "database::close_database",
            serde_json::json!({}),
        )?;
        *hlc_guard = HlcService::default();
        println!("[CLOSE_DB] HLC service reset");
    }

    // 3. Reset the per-session connection context so any leftover tx-HLC slot
    //    from the previous vault cannot leak into the next one.
    {
        let mut ctx_guard =
            state
                .connection_context
                .lock()
                .map_err(|e| DatabaseError::LockError {
                    reason: e.to_string(),
                })?;
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
pub(super) fn acquire_vault_lock_or_error(
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

    let mut guard = state
        .vault_lock
        .lock()
        .map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
    *guard = Some(lock);
    Ok(())
}

/// Reject mount attempts when this process already has a vault open.
/// Prevents dropping the live `VaultLock` (and releasing its flock) while
/// the corresponding SQLite connection is still stored in `state.db`.
///
/// `requested_path` is included in the returned error so callers (frontend,
/// test fixtures) can distinguish which mount attempt was rejected when
/// multiple are in flight.
fn reject_if_vault_already_mounted(
    state: &State<'_, AppState>,
    requested_path: &str,
) -> Result<(), DatabaseError> {
    let existing_path = {
        let lock_guard = state
            .vault_lock
            .lock()
            .map_err(|e| DatabaseError::LockError {
                reason: e.to_string(),
            })?;
        lock_guard
            .as_ref()
            .map(|lock| lock.vault_path().display().to_string())
    };
    let has_connection = state
        .db
        .0
        .lock()
        .map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?
        .is_some();
    if existing_path.is_some() || has_connection {
        return Err(DatabaseError::VaultAlreadyMountedInProcess {
            existing_path: existing_path.unwrap_or_else(|| "<unknown>".to_string()),
            requested_path: requested_path.to_string(),
        });
    }
    Ok(())
}

/// Open a `CriticalNotificationSink` against the just-mounted vault and
/// install it into `state.critical_sink`. Called from both
/// `create_encrypted_database_inner` (after migrations create the table)
/// and `open_encrypted_database` (after the table is guaranteed-present
/// by the migration check at startup). Failure to open is propagated as
/// a `DatabaseError` so the surrounding vault-mount path can unwind via
/// `close_database` — partial mount is worse than no mount.
pub(super) fn open_critical_sink(
    vault_path: &str,
    key: &str,
    state: &State<'_, AppState>,
) -> Result<(), DatabaseError> {
    let sink = crate::critical::CriticalNotificationSink::open(Path::new(vault_path), key)
        .map_err(|e| DatabaseError::DatabaseError {
            reason: format!("Failed to open critical-notification sink: {e}"),
        })?;
    let mut sink_guard = state
        .critical_sink
        .lock()
        .map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
    *sink_guard = Some(sink);
    drop(sink_guard);

    open_log_sink(vault_path, key, state)?;
    Ok(())
}

/// Open the [`crate::logging::LogSink`] against the just-mounted vault
/// and install it into `state.log_sink`. Mirrors [`open_critical_sink`]:
/// second dedicated connection to the same DB file, so the log-write
/// path survives a blocked / poisoned main mutex. See
/// `docs/plans/2026-07-21-haex-logs-no-sync.md`.
fn open_log_sink(
    vault_path: &str,
    key: &str,
    state: &State<'_, AppState>,
) -> Result<(), DatabaseError> {
    let sink = crate::logging::LogSink::open(Path::new(vault_path), key).map_err(|e| {
        DatabaseError::DatabaseError {
            reason: format!("Failed to open log sink: {e}"),
        }
    })?;
    let mut sink_guard = state
        .log_sink
        .lock()
        .map_err(|e| DatabaseError::LockError {
            reason: e.to_string(),
        })?;
    *sink_guard = Some(sink);
    Ok(())
}

/// Drop any currently-held vault lock, releasing the OS advisory lock.
/// Best-effort: a poisoned mutex here would only block future opens, which
/// is preferable to panicking in shutdown / error-recovery paths.
pub(super) fn release_vault_lock(state: &State<'_, AppState>) {
    match state.vault_lock.lock() {
        Ok(mut guard) => {
            *guard = None;
        }
        Err(e) => {
            eprintln!("[CLOSE_DB] Warning: vault_lock mutex poisoned, skipping release: {e}");
        }
    }
}

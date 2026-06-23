// src-tauri/src/database/core/init.rs

use crate::crdt::hlc::HlcService;
use crate::crdt::trigger::{HLC_FUNCTION_NAME, UUID_FUNCTION_NAME};
use crate::database::connection_context::ConnectionContext;
use crate::database::error::DatabaseError;
use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OpenFlags};
use uuid::Uuid;

/// Öffnet und initialisiert eine Datenbank mit Verschlüsselung.
///
/// Registers the `gen_uuid` and `current_hlc` UDFs and wires commit/rollback
/// hooks so the transaction-scoped HLC slot is cleared at the end of every
/// transaction.
pub fn open_and_init_db(
    path: &str,
    key: &str,
    create: bool,
    hlc_service: HlcService,
    context: ConnectionContext,
) -> Result<Connection, DatabaseError> {
    let flags = if create {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };

    let conn =
        Connection::open_with_flags(path, flags).map_err(|e| DatabaseError::ConnectionFailed {
            path: path.to_string(),
            reason: e.to_string(),
        })?;

    conn.pragma_update(None, "key", key)
        .map_err(|e| DatabaseError::PragmaError {
            pragma: "key".to_string(),
            reason: e.to_string(),
        })?;

    // Enable foreign key constraints
    // This must be set for PRAGMA defer_foreign_keys to work
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| DatabaseError::PragmaError {
            pragma: "foreign_keys".to_string(),
            reason: e.to_string(),
        })?;

    // Verify foreign keys are enabled
    let fk_enabled: i32 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|e| DatabaseError::PragmaError {
            pragma: "foreign_keys (verify)".to_string(),
            reason: e.to_string(),
        })?;

    if fk_enabled == 1 {
        println!("✅ Foreign key constraints enabled.");
    } else {
        eprintln!("❌ Failed to enable foreign key constraints.");
    }

    // Register custom UUID function for SQLite triggers
    conn.create_scalar_function(
        UUID_FUNCTION_NAME,
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
        |_ctx| Ok(Uuid::new_v4().to_string()),
    )
    .map_err(|e| DatabaseError::DatabaseError {
        reason: format!("Failed to register {UUID_FUNCTION_NAME} function: {e}"),
    })?;

    // Register transaction-scoped HLC UDF. All calls within a single SQLite
    // transaction (explicit or auto-commit) return the same timestamp.
    register_current_hlc_udf(&conn, hlc_service, context.clone())?;
    install_tx_hlc_hooks(&conn, context)?;

    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode=WAL;", [], |row| row.get(0))
        .map_err(|e| DatabaseError::PragmaError {
            pragma: "journal_mode=WAL".to_string(),
            reason: e.to_string(),
        })?;

    if journal_mode.eq_ignore_ascii_case("wal") {
        println!("WAL mode successfully enabled.");
    } else {
        eprintln!("Failed to enable WAL mode, journal_mode is '{journal_mode}'.");
    }

    Ok(conn)
}

/// Registers the `current_hlc()` UDF on a connection. Extracted so tests that
/// create bare in-memory connections can use the same registration logic.
pub fn register_current_hlc_udf(
    conn: &Connection,
    hlc_service: HlcService,
    context: ConnectionContext,
) -> Result<(), DatabaseError> {
    // Flags explained:
    // - UTF8: default string encoding for TEXT args/return.
    // - INNOCUOUS: safe to call from trigger/view context when
    //   `trusted_schema=OFF` (no side effects, no access to attacker-
    //   controlled state).
    // - DETERMINISTIC: zero-arg deterministic functions are constant-folded
    //   by the query planner, so two `current_hlc()` calls inside the same
    //   statement evaluate to the same value even without our slot cache.
    //   The cache covers the cross-statement case inside a write tx.
    conn.create_scalar_function(
        HLC_FUNCTION_NAME,
        0,
        FunctionFlags::SQLITE_UTF8
            | FunctionFlags::SQLITE_INNOCUOUS
            | FunctionFlags::SQLITE_DETERMINISTIC,
        move |_ctx| {
            context
                .current_or_new_tx_hlc(&hlc_service)
                .map(|ts| ts.to_string())
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))
        },
    )
    .map_err(|e| DatabaseError::DatabaseError {
        reason: format!("Failed to register {HLC_FUNCTION_NAME} function: {e}"),
    })
}

/// Wires commit_hook, rollback_hook and update_hook so the per-transaction
/// HLC slot behaves correctly:
/// - commit_hook / rollback_hook: clear the slot and the write-pending flag at
///   the end of every transaction.
/// - update_hook: flip the write-pending flag on the first row-level
///   INSERT/UPDATE/DELETE in a transaction, so that a stray read-only
///   `SELECT current_hlc()` cannot poison the HLC of a later write.
pub fn install_tx_hlc_hooks(
    conn: &Connection,
    context: ConnectionContext,
) -> Result<(), DatabaseError> {
    let ctx_commit = context.clone();
    conn.commit_hook(Some(move || {
        ctx_commit.reset_tx_slot();
        false
    }))
    .map_err(|e| DatabaseError::DatabaseError {
        reason: format!("Failed to install commit_hook: {e}"),
    })?;

    let ctx_rollback = context.clone();
    conn.rollback_hook(Some(move || {
        ctx_rollback.reset_tx_slot();
    }))
    .map_err(|e| DatabaseError::DatabaseError {
        reason: format!("Failed to install rollback_hook: {e}"),
    })?;

    let ctx_update = context;
    conn.update_hook(Some(
        move |_action, _db: &str, _table: &str, _row_id: i64| {
            ctx_update.mark_write_pending();
        },
    ))
    .map_err(|e| DatabaseError::DatabaseError {
        reason: format!("Failed to install update_hook: {e}"),
    })?;
    Ok(())
}

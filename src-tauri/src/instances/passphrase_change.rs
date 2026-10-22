//! `change_vault_passphrase` — re-keys the active vault's file on this device (spec 042 US3,
//! FR-011 to FR-015, research R4).
//!
//! The passphrase only keys this device's SQLCipher file; every linked device keeps its own and
//! sync never sees it, so nothing else changes. holzi does not keep the passphrase after opening,
//! so the current one is verified against the file itself. The command is not in the action
//! catalog (`src/lib/actions`), so no agent can call it (FR-014).

use std::path::Path;

use haex_crdt::rusqlite::{Connection, ErrorCode, OpenFlags};
use serde::Deserialize;
use tauri::{AppHandle, Runtime, State};
use ts_rs::TS;

use crate::chat::session::ChatState;
use crate::error::{HolziError, Result};
use crate::state::AppState;

use super::passphrase::Passphrase;
use super::paths::get_instance_path;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ChangePassphraseArgs {
    #[ts(type = "string")]
    pub current: Passphrase,
    #[ts(type = "string")]
    pub new: Passphrase,
}

/// Everything `change_vault_passphrase` does, generic over `R: Runtime` so a test can run it over
/// `tauri::test::MockRuntime`. Holds the chat/vault operation lock, so no turn, model load or open
/// runs meanwhile; the re-key itself runs on the vault gate's blocking pool, so a close waits for
/// it, and under the database's only connection, so sync and every other access wait too (FR-015).
pub async fn change_vault_passphrase_core<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    chat: &ChatState,
    current: Passphrase,
    new: Passphrase,
) -> Result<()> {
    let _operation = chat.acquire_operation()?;
    new.validate_new()?;
    if new.as_str() == current.as_str() {
        return Err(HolziError::WeakPassphrase {
            reason: "the new passphrase must differ from the current one".into(),
        });
    }
    let db = state.database()?;
    let name = state.active_name()?.ok_or(HolziError::NoActiveInstance)?;
    let path = get_instance_path(app, &name)?;

    state
        .gate()
        .spawn_blocking(move || -> Result<()> {
            verify_current(&path, &current)?;
            // Journal-mode switches and `PRAGMA rekey` cannot run in a CRDT transaction.
            #[allow(clippy::disallowed_methods)]
            let rekeyed = db.with_connection(|conn| rekey(conn, &new));
            Ok(rekeyed?)
        })?
        .await
        .map_err(|e| HolziError::CrdtInit {
            reason: format!("passphrase change task failed: {e}"),
        })?
}

/// Opens a second, read-only connection with `current`, the same way haex-crdt keys its own (only
/// `PRAGMA key`, SQLCipher defaults otherwise). It is closed again before the re-key, which cannot
/// leave WAL mode while another connection is open. The External Bridge checks the vault password
/// of a passkey confirmation with it too (spec 049, research R12).
pub(crate) fn verify_current(path: &Path, current: &Passphrase) -> haex_crdt::Result<()> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.pragma_update(None, "key", current.as_str())?;
    match conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(_) => Ok(()),
        Err(e) if e.sqlite_error_code() == Some(ErrorCode::NotADatabase) => {
            Err(HolziError::WrongPassphrase.into())
        }
        Err(e) => Err(e.into()),
    }
}

/// The sequence from haex-vault 2.x (Legacy) (`database/maintenance.rs`, `change_vault_password`): SQLCipher's
/// `rekey` does not work in WAL mode, so the WAL is folded into the file and the journal switched
/// to DELETE first, whose rollback journal also makes the re-key atomic.
fn rekey(conn: &Connection, new: &Passphrase) -> haex_crdt::Result<()> {
    let busy: i64 = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
    if busy != 0 {
        return Err(HolziError::InvalidInput {
            reason: "the vault is busy, try again".into(),
        }
        .into());
    }
    conn.pragma_update_and_check(None, "journal_mode", "DELETE", |r| r.get::<_, String>(0))?;
    // ponytail: rusqlite renders the key into the text of a `PRAGMA rekey` statement that is not
    // zeroed, like `PRAGMA key` on open (`passwords/import/legacy_haex_vault/open.rs`). Ceiling: a plain
    // copy of the new passphrase in freed memory until it is reused. Upgrade path:
    // `sqlite3_rekey_v2` through `rusqlite::ffi` with a zeroizing buffer.
    let rekeyed = conn.pragma_update(None, "rekey", new.as_str());
    // Back to WAL in any case, so a failed re-key leaves the session as it was.
    let wal = conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0));
    rekeyed?;
    wal?;
    Ok(())
}

/// Changes the passphrase of the active vault on this device.
#[tauri::command]
pub async fn change_vault_passphrase(
    app: AppHandle,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    args: ChangePassphraseArgs,
) -> Result<()> {
    let ChangePassphraseArgs { current, new } = args;
    change_vault_passphrase_core(&app, &state, &chat, current, new).await
}

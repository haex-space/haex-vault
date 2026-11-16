//! Where the name of the assistant lives in the vault (design 2026-11-16): the vault preference
//! `assistant.name`, one row for every device. [`follow`] keeps [`current`] (what the speech output
//! reads) in step with the open vault, also when another device changes the name.

use haex_crdt::CrdtTransaction;
use tauri::{AppHandle, Runtime, State};
use tokio::sync::broadcast::error::RecvError;

use crate::assistant_name::{current, set_current, AssistantName, PREF_KEY};
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// The name of the vault; the default while it has none or an unusable one.
pub fn read(q: &mut impl Query) -> haex_crdt::Result<AssistantName> {
    let stored = preferences::get(q, PrefScope::Vault, PREF_KEY)?;
    Ok(AssistantName::from_stored(stored.as_deref()))
}

/// Keeps `name` in the vault. The caller has checked it.
pub fn write(tx: &mut CrdtTransaction<'_>, name: &AssistantName) -> haex_crdt::Result<()> {
    let text = serde_json::to_string(name)
        .map_err(|error| haex_crdt::Error::consumer(error.to_string()))?;
    preferences::insert_or_update(tx, PrefScope::Vault, PREF_KEY, &text)?;
    Ok(())
}

/// Drops the row, so the vault has the default again.
pub fn reset(tx: &mut CrdtTransaction<'_>) -> haex_crdt::Result<()> {
    preferences::delete(tx, PrefScope::Vault, PREF_KEY)?;
    Ok(())
}

/// Mirrors the open vault's name into [`current`] now and after every change of the preferences,
/// until the session ends.
pub fn follow<R: Runtime>(_app: &AppHandle<R>, state: &AppState) {
    // Never the name of the vault that was open before, not even until the first read.
    set_current(AssistantName::default());
    let db = match state.database() {
        Ok(db) => db,
        Err(error) => {
            log::warn!("assistant name: not followed: {error}");
            return;
        }
    };
    let mut changes = state.vault_changes().subscribe();
    let gate = state.gate().clone();
    let token = gate.token();
    let follow = async move {
        // Read once after subscribing, so a change racing the start is read here or delivered below.
        refresh(&db).await;
        loop {
            let reload = tokio::select! {
                () = token.cancelled() => return,
                received = changes.recv() => match received {
                    Ok(tables) => tables.iter().any(|table| table == "preferences"),
                    Err(RecvError::Lagged(_)) => true,
                    Err(RecvError::Closed) => return,
                },
            };
            if reload {
                refresh(&db).await;
            }
        }
    };
    if let Err(error) = gate.spawn(follow) {
        log::warn!("assistant name: not followed: {error}");
    }
}

async fn refresh(db: &VaultDb) {
    match db.read(|q| read(q)).await {
        Ok(name) => {
            if name != current() {
                set_current(name);
            }
        }
        Err(error) => log::warn!("assistant name: could not be read: {error}"),
    }
}

/// The name of the open vault.
#[tauri::command]
pub async fn assistant_name_get(state: State<'_, AppState>) -> Result<AssistantName> {
    let db = active_database(&state)?;
    db.read(|q| read(q)).await
}

/// Checks and keeps a new name for the whole vault; answers it as it counts. An empty `spoken`
/// is said as written.
#[tauri::command]
pub async fn assistant_name_set(
    state: State<'_, AppState>,
    written: String,
    spoken: String,
) -> Result<AssistantName> {
    let name = AssistantName::new(&written, &spoken).map_err(|error| HolziError::InvalidInput {
        reason: error.to_string(),
    })?;
    let db = active_database(&state)?;
    let kept = name.clone();
    db.write(move |tx| write(tx, &kept)).await?;
    set_current(name.clone());
    Ok(name)
}

/// Back to the default name "Haex", said "Hex".
#[tauri::command]
pub async fn assistant_name_reset(state: State<'_, AppState>) -> Result<AssistantName> {
    let db = active_database(&state)?;
    db.write(reset).await?;
    set_current(AssistantName::default());
    Ok(AssistantName::default())
}

#[cfg(test)]
#[path = "assistant_name_vault_tests.rs"]
mod tests;

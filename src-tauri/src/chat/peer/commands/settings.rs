//! The commands of the chat settings (spec 072 FR-080, contracts/ipc.md). The settings themselves
//! are vault preferences, read and written through the preferences commands like the other
//! settings; what needs the backend is the test of a Blossom-Server address.

use nostr::key::Keys;
use serde::Deserialize;
use tauri::State;

use crate::chat::peer::files::probe::{self, Check};
use crate::chat::peer::files::upload_key::{self, ChatUploadKey, Resolved};
use crate::error::{HaexError, Result};
use crate::passwords::service::PasswordsService;
use crate::remote_storage::address::SystemResolver;
use crate::remote_storage::model::CredentialsCandidate;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::vault_gate::VaultDb;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlossomCheckArgs {
    pub url: String,
}

/// Asks the Blossom-Server at `url` whether it takes chat files (research R12), signing as an
/// upload does: with the upload key when one is set (FR-081), else with a fresh key.
#[tauri::command]
pub async fn chat_blossom_check(
    state: State<'_, AppState>,
    args: BlossomCheckArgs,
) -> Result<Check> {
    let db = active_database(&state)?;
    let signer = match upload_key::resolve(&PasswordsService::new(db.clone()), &db).await? {
        Resolved::Unset => Keys::generate(),
        Resolved::Ready(keys, _) => keys,
        Resolved::Missing => return Err(key_unavailable("chat.file.keyMissing")),
        Resolved::Syncing => return Err(key_unavailable("chat.file.keySyncing")),
    };
    Ok(probe::check(&args.url, &SystemResolver, &signer).await)
}

fn key_unavailable(reason: &str) -> HaexError {
    HaexError::InvalidInput {
        reason: reason.to_owned(),
    }
}

fn passwords(state: &State<'_, AppState>) -> Result<(PasswordsService, VaultDb)> {
    let db = active_database(state)?;
    Ok((PasswordsService::new(db.clone()), db))
}

/// The Blossom upload key as the settings show it: where it stands and its public key to register
/// at the server (FR-081). No secret leaves the backend.
#[tauri::command]
pub async fn chat_blossom_key(state: State<'_, AppState>) -> Result<ChatUploadKey> {
    let (passwords, db) = passwords(&state)?;
    upload_key::describe(&passwords, &db).await
}

/// Makes a new upload key, keeps it as an entry of the password manager and sets it.
#[tauri::command]
pub async fn chat_blossom_key_create(state: State<'_, AppState>) -> Result<ChatUploadKey> {
    let (passwords, db) = passwords(&state)?;
    upload_key::create(&passwords, &db).await?;
    upload_key::describe(&passwords, &db).await
}

/// The entries of the password manager that hold a key and may be chosen.
#[tauri::command]
pub async fn chat_blossom_key_candidates(
    state: State<'_, AppState>,
) -> Result<Vec<CredentialsCandidate>> {
    upload_key::candidates(&active_database(&state)?).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlossomKeyChooseArgs {
    pub item_id: String,
}

/// Sets an entry of the user that holds a key as the upload key.
#[tauri::command]
pub async fn chat_blossom_key_choose(
    state: State<'_, AppState>,
    args: BlossomKeyChooseArgs,
) -> Result<ChatUploadKey> {
    let (passwords, db) = passwords(&state)?;
    upload_key::choose(&passwords, &db, &args.item_id).await?;
    upload_key::describe(&passwords, &db).await
}

/// Unsets the upload key, keeping the entry for cleanup of already uploaded files.
#[tauri::command]
pub async fn chat_blossom_key_remove(state: State<'_, AppState>) -> Result<ChatUploadKey> {
    let (passwords, db) = passwords(&state)?;
    upload_key::remove(&db).await?;
    upload_key::describe(&passwords, &db).await
}

//! The Tauri commands of the settings category "Cloud-Speicher" (spec 038, contracts/tauri-commands.md).
//! Each one calls the [`StorageService`] over the open vault; none returns a secret. The dialog
//! command for extensions (`storage_dialog_resolve`) comes with the bridge (PR D).

use std::sync::{Arc, OnceLock};

use tauri::State;

use super::address::SystemResolver;
use super::model::{
    ConnectionInput, ConnectionView, RemovalPreview, RemovalTarget, StorageInput, StorageOverview,
    StorageView, TestResult,
};
use super::s3::S3Store;
use super::service::StorageService;
use super::RemoteStore;
use crate::error::Result;
use crate::extensions::remote_storage_dialog::{StorageAnswer, StorageTrial};
use crate::passwords::service::PasswordsService;
use crate::state::AppState;

/// The provider of this process: S3 with the resolver of the operating system.
pub fn s3_store() -> Arc<dyn RemoteStore> {
    static STORE: OnceLock<Arc<S3Store>> = OnceLock::new();
    STORE.get_or_init(|| Arc::new(S3Store::system())).clone()
}

/// The service over the active vault; `VaultClosed` or `NoActiveInstance` when there is none.
pub(crate) fn service(state: &AppState) -> Result<StorageService> {
    let db = state.database()?;
    let passwords = PasswordsService::with_usage(db.clone(), state.usage());
    Ok(StorageService::new(
        db,
        passwords,
        s3_store(),
        Arc::new(SystemResolver),
    ))
}

/// Lists connections and storages with credential availability and local test results.
#[tauri::command]
pub async fn storage_list(state: State<'_, AppState>) -> Result<StorageOverview> {
    service(&state)?.overview().await
}

/// Creates or updates a connection, testing new credentials or changed address data first.
#[tauri::command]
pub async fn storage_connection_save(
    state: State<'_, AppState>,
    input: ConnectionInput,
) -> Result<ConnectionView> {
    service(&state)?.save_connection(input).await
}

/// Removes a connection, its storages and permissions, then any unused credentials.
#[tauri::command]
pub async fn storage_connection_remove(state: State<'_, AppState>, id: String) -> Result<()> {
    service(&state)?.remove_connection(&id).await
}

/// Creates or updates a storage, testing access before saving a new or changed bucket.
#[tauri::command]
pub async fn storage_save(state: State<'_, AppState>, input: StorageInput) -> Result<StorageView> {
    service(&state)?.save_storage(input).await
}

/// Removes a storage together with its extension permissions and local test result.
#[tauri::command]
pub async fn storage_remove(state: State<'_, AppState>, id: String) -> Result<()> {
    service(&state)?.remove_storage(&id).await
}

/// Returns the storage names and extension IDs affected by a proposed removal.
#[tauri::command]
pub async fn storage_removal_preview(
    state: State<'_, AppState>,
    target: RemovalTarget,
) -> Result<RemovalPreview> {
    service(&state)?.removal_preview(target).await
}

/// Tests access to a storage and returns the outcome and any test object left behind.
#[tauri::command]
pub async fn storage_test(state: State<'_, AppState>, id: String) -> Result<TestResult> {
    service(&state)?.test_storage(&id).await
}

/// Answers a storage dialog of an extension (research R6). Credentials in the answer come from
/// holzi's window over the whole app and stay in Rust; for them it waits for their test: a failed
/// one keeps the dialog open for corrected credentials.
#[tauri::command]
pub async fn storage_dialog_resolve(
    state: State<'_, AppState>,
    request_id: String,
    answer: StorageAnswer,
) -> Result<StorageTrial> {
    let trial = state.extensions().storage.resolve(&request_id, answer);
    Ok(match trial {
        Some(trial) => trial.await.unwrap_or(StorageTrial::Ended),
        None => StorageTrial::Ended,
    })
}

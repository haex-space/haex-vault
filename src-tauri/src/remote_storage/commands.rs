//! The Tauri commands of the settings category "Cloud-Speicher" (spec 038, contracts/tauri-commands.md).
//! Each one calls the [`StorageService`] over the open vault; none returns a secret. The dialog
//! command for extensions (`storage_dialog_resolve`) comes with the bridge (PR D).

use std::sync::{Arc, OnceLock};

use tauri::State;

use super::address::SystemResolver;
use super::model::{
    ConnectionInput, ConnectionView, CredentialsCandidate, GeneratedKey, RemovalPreview,
    RemovalTarget, StorageInput, StorageOverview, StorageView, TestResult,
};
use super::protocol::ProtocolStore;
use super::s3::S3Store;
use super::service::StorageService;
use super::sftp::{keys, SftpStore};
use super::webdav::WebDavStore;
use super::{credentials, Protocol, RemoteStore};
use crate::error::Result;
use crate::extensions::remote_storage_dialog::{StorageAnswer, StorageTrial};
use crate::passwords::service::PasswordsService;
use crate::state::AppState;

/// The providers of this process (spec 050 research R1): one store per protocol, each with the
/// resolver of the operating system, chosen per call by the connection's protocol.
pub fn stores() -> Arc<dyn RemoteStore> {
    static STORE: OnceLock<Arc<ProtocolStore>> = OnceLock::new();
    STORE
        .get_or_init(|| {
            Arc::new(ProtocolStore::new(
                Arc::new(S3Store::system()),
                Some(Arc::new(WebDavStore::system())),
                Some(Arc::new(SftpStore::system())),
            ))
        })
        .clone()
}

/// The service over the active vault; `VaultClosed` or `NoActiveInstance` when there is none.
pub(crate) fn service(state: &AppState) -> Result<StorageService> {
    let db = state.database()?;
    let passwords = PasswordsService::with_usage(db.clone(), state.usage());
    Ok(StorageService::new(
        db,
        passwords,
        stores(),
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

/// The entries of the password manager that fit `protocol`, to choose as credentials (spec 052);
/// titles, user names and addresses only.
#[tauri::command]
pub async fn storage_credentials_candidates(
    state: State<'_, AppState>,
    protocol: Protocol,
) -> Result<Vec<CredentialsCandidate>> {
    credentials::candidates(&state.database()?, protocol).await
}

/// Creates the missing folder of a WebDAV or SFTP storage on the user's word, then tests it
/// (spec 050 FR-006).
#[tauri::command]
pub async fn storage_folder_create(state: State<'_, AppState>, id: String) -> Result<TestResult> {
    service(&state)?.create_folder(&id).await
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

/// Generates an SSH key for an SFTP connection (spec 050 research R4): the id the window sends
/// back on save and the public key line for the provider. The private key stays in the process
/// until the connection is saved, for 30 minutes at most, and goes when the vault closes.
#[tauri::command]
pub async fn storage_ssh_key_generate(state: State<'_, AppState>) -> Result<GeneratedKey> {
    let gate = state.gate();
    let token = gate.token();
    static WATCHED: std::sync::Once = std::sync::Once::new();
    WATCHED.call_once(|| {
        tokio::spawn(async move {
            token.cancelled().await;
            keys::pending().clear();
        });
    });
    let (key_id, public_key) =
        keys::pending()
            .generate()
            .map_err(|_| crate::error::HolziError::StorageInvalid {
                field: "credentials".to_owned(),
            })?;
    Ok(GeneratedKey { key_id, public_key })
}

/// Takes the host key an SFTP server showed in place of the confirmed one (research R5).
#[tauri::command]
pub async fn storage_host_key_accept(
    state: State<'_, AppState>,
    connection_id: String,
    host_key: String,
) -> Result<ConnectionView> {
    service(&state)?
        .accept_host_key(&connection_id, &host_key)
        .await
}

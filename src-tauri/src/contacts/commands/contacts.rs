//! Commands of the window for contacts and groups (spec 051, contracts/tauri-commands.md
//! §Kontakte). A photo travels as the choice of the system's open dialog, never as bytes through
//! the webview; only the stored JPEG goes back, as raw bytes.

use serde::Deserialize;
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use ts_rs::TS;

use super::service;
use crate::contacts::access::Caller;
use crate::contacts::model::{Contact, ContactHeader, ContactInput, ContactPatch, Group};
use crate::error::{HolziError, Result};
use crate::files::PickedFile;
use crate::passwords::binaries::read_attachment_file;
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactIdArgs {
    pub id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactCreateArgs {
    pub input: ContactInput,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactUpdateArgs {
    pub id: String,
    pub patch: ContactPatch,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactsDeleteArgs {
    pub ids: Vec<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactPhotoArgs {
    pub id: String,
    /// The choice of the open dialog; absent removes the photo.
    pub file: Option<PickedFile>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactPhotoHashArgs {
    pub hash: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct ContactGroupSaveArgs {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactGroupMembersArgs {
    pub group_id: String,
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

/// The headers of every contact.
#[tauri::command]
pub async fn contacts_list(state: State<'_, AppState>) -> Result<Vec<ContactHeader>> {
    service(&state)?.list(&Caller::User, &[]).await
}

/// One contact with its values and groups.
#[tauri::command]
pub async fn contacts_get(state: State<'_, AppState>, args: ContactIdArgs) -> Result<Contact> {
    service(&state)?.get(&Caller::User, &[], &args.id).await
}

/// Creates a contact.
#[tauri::command]
pub async fn contacts_create(
    state: State<'_, AppState>,
    args: ContactCreateArgs,
) -> Result<Contact> {
    service(&state)?
        .create(&Caller::User, &[], args.input)
        .await
}

/// Changes the fields and values the patch names.
#[tauri::command]
pub async fn contacts_update(
    state: State<'_, AppState>,
    args: ContactUpdateArgs,
) -> Result<Contact> {
    service(&state)?
        .update(&Caller::User, &[], &args.id, args.patch)
        .await
}

/// Deletes contacts (the window asks first, FR-002).
#[tauri::command]
pub async fn contacts_delete(state: State<'_, AppState>, args: ContactsDeleteArgs) -> Result<()> {
    service(&state)?.delete(&Caller::User, &[], args.ids).await
}

/// Sets the photo from the chosen file, or removes it.
#[tauri::command]
pub async fn contacts_set_photo(
    app: AppHandle,
    state: State<'_, AppState>,
    args: ContactPhotoArgs,
) -> Result<Contact> {
    let bytes = match args.file {
        Some(file) => {
            tauri::async_runtime::spawn_blocking(move || read_attachment_file(&app, &file))
                .await
                .map_err(|e| HolziError::Io {
                    reason: format!("photo read: {e}"),
                })??
                .1
        }
        None => Vec::new(),
    };
    service(&state)?
        .set_photo(&Caller::User, &args.id, bytes)
        .await
}

/// The JPEG of a stored photo, as raw bytes.
#[tauri::command]
pub async fn contacts_photo(
    state: State<'_, AppState>,
    args: ContactPhotoHashArgs,
) -> Result<Response> {
    let bytes = service(&state)?
        .photo(&Caller::User, &[], &args.hash)
        .await?;
    Ok(Response::new(bytes))
}

/// Every group.
#[tauri::command]
pub async fn contacts_groups(state: State<'_, AppState>) -> Result<Vec<Group>> {
    service(&state)?.groups(&Caller::User).await
}

/// Creates or renames a group.
#[tauri::command]
pub async fn contacts_group_save(
    state: State<'_, AppState>,
    args: ContactGroupSaveArgs,
) -> Result<Group> {
    service(&state)?
        .group_save(&Caller::User, args.id, args.name)
        .await
}

/// Deletes a group; its contacts stay.
#[tauri::command]
pub async fn contacts_group_delete(state: State<'_, AppState>, args: ContactIdArgs) -> Result<()> {
    service(&state)?.group_delete(&Caller::User, args.id).await
}

/// Adds contacts to a group and removes others from it.
#[tauri::command]
pub async fn contacts_group_members(
    state: State<'_, AppState>,
    args: ContactGroupMembersArgs,
) -> Result<()> {
    service(&state)?
        .group_members(&Caller::User, args.group_id, args.add, args.remove)
        .await
}

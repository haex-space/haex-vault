//! Attachment commands (spec 034, US5, `contracts/tauri-commands.md` §Anhänge). Files travel as
//! the choice of the system's dialogs (spec 043: a path or a provider address), never as bytes
//! through the webview; only the bytes of an image
//! preview go back, as raw bytes (`tauri::ipc::Response`), and the text of a text attachment for
//! the attachment view (spec 058); a PDF reaches the view as a URL of the media server.

use std::sync::Arc;

use serde::Deserialize;
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use ts_rs::TS;

use super::service;
use crate::error::Result;
use crate::files::local::text::TextContent;
use crate::files::media::MediaServer;
use crate::files::streaming::BytesSource;
use crate::files::PickedFile;
use crate::passwords::access::Caller;
use crate::passwords::model::AttachmentView;
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct AttachmentAddArgs {
    pub item_id: String,
    /// The choice of the open dialog.
    pub file: PickedFile,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRenameArgs {
    pub attachment_id: String,
    pub file_name: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct AttachmentIdArgs {
    pub attachment_id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct AttachmentOpenArgs {
    pub attachment_id: String,
    /// The tab of the view; its URL ends when the tab closes.
    pub tab_id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct AttachmentSaveArgs {
    pub attachment_id: String,
    /// The choice of the save dialog.
    pub file: PickedFile,
}

/// Attaches a file; above 25 MiB it is refused before it is read.
#[tauri::command]
pub async fn passwords_attachment_add(
    app: AppHandle,
    state: State<'_, AppState>,
    args: AttachmentAddArgs,
) -> Result<AttachmentView> {
    service(&state)?
        .attachment_add(&Caller::User, args.item_id, app, args.file)
        .await
}

/// Renames an attachment on its entry; returns the stored name.
#[tauri::command]
pub async fn passwords_attachment_rename(
    state: State<'_, AppState>,
    args: AttachmentRenameArgs,
) -> Result<String> {
    service(&state)?
        .attachment_rename(&Caller::User, args.attachment_id, args.file_name)
        .await
}

/// Removes the link of an attachment (the data stays until the clean-up).
#[tauri::command]
pub async fn passwords_attachment_remove(
    state: State<'_, AppState>,
    args: AttachmentIdArgs,
) -> Result<()> {
    service(&state)?
        .attachment_remove(&Caller::User, args.attachment_id)
        .await
}

/// Writes an attachment byte for byte to the chosen file.
#[tauri::command]
pub async fn passwords_attachment_save(
    app: AppHandle,
    state: State<'_, AppState>,
    args: AttachmentSaveArgs,
) -> Result<()> {
    service(&state)?
        .attachment_save(&Caller::User, args.attachment_id, app, args.file)
        .await
}

/// The bytes of an image attachment; anything that is not an image is `not_previewable`.
#[tauri::command]
pub async fn passwords_attachment_preview(
    state: State<'_, AppState>,
    args: AttachmentIdArgs,
) -> Result<Response> {
    let bytes = service(&state)?
        .attachment_preview(&Caller::User, args.attachment_id)
        .await?;
    Ok(Response::new(bytes))
}

/// The text of a text attachment for the attachment view (spec 058 FR-003).
#[tauri::command]
pub async fn passwords_attachment_text(
    state: State<'_, AppState>,
    args: AttachmentIdArgs,
) -> Result<TextContent> {
    service(&state)?
        .attachment_text(&Caller::User, args.attachment_id)
        .await
}

/// A PDF attachment for the attachment view (spec 058 FR-004): its bytes go to the media server,
/// the answer is their URL. The window releases it with `files_release`; it also ends with the tab
/// and with the vault.
#[tauri::command]
pub async fn passwords_attachment_open(
    state: State<'_, AppState>,
    media: State<'_, MediaServer>,
    args: AttachmentOpenArgs,
) -> Result<String> {
    let bytes = service(&state)?
        .attachment_pdf(&Caller::User, args.attachment_id)
        .await?;
    Ok(media.register(
        &args.tab_id,
        Arc::new(BytesSource::new(Arc::from(bytes), "application/pdf")),
    ))
}

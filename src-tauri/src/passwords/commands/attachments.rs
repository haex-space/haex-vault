//! Attachment commands (spec 034, US5, `contracts/tauri-commands.md` §Anhänge). Files travel as
//! the choice of the system's dialogs (spec 043: a path or a provider address), never as bytes
//! through the webview; only the bytes of an image
//! preview go back, as raw bytes (`tauri::ipc::Response`), and the text of a text attachment for
//! the attachment view (spec 058); a PDF reaches the view as a URL of the media server, and on the
//! desktop any attachment opens with the system's app after the notice of spec 048.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use ts_rs::TS;

use super::service;
use crate::error::{HolziError, Result};
use crate::files::local::text::TextContent;
use crate::files::media::MediaServer;
use crate::files::state::FilesState;
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
pub struct AttachmentOpenSystemArgs {
    pub attachment_id: String,
    /// The user confirmed the notice about the decrypted copy.
    pub confirmed: bool,
    /// "Nicht mehr fragen" was ticked in the notice (this device only).
    pub dont_ask_again: bool,
}

/// What "Mit System-App öffnen" did (spec 058 FR-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub enum OpenSystemOutcome {
    /// The copy was written and the system's app started.
    Opened,
    /// The notice has to be shown first; nothing was read.
    NeedsNotice,
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

/// Opens an attachment with the system's app (spec 058 FR-009 to FR-012): without `confirmed`, and
/// unless "Nicht mehr fragen" was chosen on this device, the answer is `needsNotice`. The copy lies
/// below the folder of spec 048 for decrypted copies, which goes when the vault closes and after a
/// crash; when no app opens it, it goes at once. Not on mobiles (spec 044 R11).
#[tauri::command]
pub async fn passwords_attachment_open_system(
    app: AppHandle,
    state: State<'_, AppState>,
    files: State<'_, FilesState>,
    args: AttachmentOpenSystemArgs,
) -> Result<OpenSystemOutcome> {
    #[cfg(mobile)]
    {
        let _ = (app, state, files, args);
        Err(HolziError::InvalidInput {
            reason: "unsupported".to_owned(),
        })
    }
    #[cfg(desktop)]
    {
        use tauri_plugin_opener::OpenerExt;
        // Arms the clean-up on close also when the file browser never ran in this session.
        files.forget_on_close(state.gate());
        let db = crate::state_utils::active_database(&state)?;
        let device = crate::device::commands::resolve_vault_device_uuid(&app, &db).await?;
        let copy = service(&state)?
            .attachment_copy_for_app(
                &Caller::User,
                args.attachment_id,
                files.opened_encrypted.clone(),
                device,
                args.confirmed,
                args.dont_ask_again,
            )
            .await?;
        let Some(path) = copy else {
            return Ok(OpenSystemOutcome::NeedsNotice);
        };
        if let Err(error) = app.opener().open_path(path.to_string_lossy(), None::<&str>) {
            // The opener's error can quote the path (FR-013).
            let _ = error;
            log::warn!("passwords: the system could not open an attachment");
            crate::passwords::attachment_view::remove_copy(&path).await;
            return Err(HolziError::InvalidInput {
                reason: "no_app".to_owned(),
            });
        }
        Ok(OpenSystemOutcome::Opened)
    }
}

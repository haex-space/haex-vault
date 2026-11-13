//! The commands that import a pass from a file and propose where it goes (spec 092,
//! `contracts/tauri-commands.md`). Each is `Caller::User` and calls only the service; the file is
//! read into memory, never extracted, and no address in it is called (FR-029, FR-035).

use serde::Deserialize;
use tauri::{AppHandle, State};
use ts_rs::TS;

use super::service;
use crate::error::{HolziError, Result};
use crate::files::picked::{self, PickedFile};
use crate::passwords::access::Caller;
use crate::passwords::pass::import_model::{
    PassImportChoices, PassImportPreview, PassImportReport, PassImportSaved,
};
use crate::passwords::pass::import_report::render_text;
use crate::passwords::pass::model::PassKind;
use crate::passwords::pass::pkpass;
use crate::passwords::pass::place::Proposal;
use crate::passwords::pass::signature::{self, SignatureResult, SignatureState};
use crate::state::AppState;

/// The largest pass file read: a real one is a few hundred kilobytes.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct PassPlaceArgs {
    #[ts(type = "string")]
    pub pass_kind: PassKind,
    pub has_date: bool,
    pub provider: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct PassImportPreviewArgs {
    pub file: PickedFile,
    /// The language of the window (`de`, `en`): the texts of the pass in that language are used.
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PassImportSaveArgs {
    pub preview_id: String,
    pub choices: PassImportChoices,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PassImportCancelArgs {
    pub preview_id: String,
}

/// The place proposed for a new pass from a file or a scan (FR-019). Creates nothing.
#[tauri::command]
pub async fn passwords_pass_place_proposal(
    state: State<'_, AppState>,
    args: PassPlaceArgs,
) -> Result<Proposal> {
    service(&state)?
        .pass_place_proposal(&Caller::User, args.pass_kind, args.has_date, args.provider)
        .await
}

/// Reads a pass file into memory and returns what the dialog shows; a file that is no pass is
/// refused with the reason and leaves nothing behind (FR-033).
#[tauri::command]
pub async fn passwords_pass_import_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    args: PassImportPreviewArgs,
) -> Result<PassImportPreview> {
    let service = service(&state)?;
    let bytes = picked::read(&app, &args.file, MAX_FILE_BYTES)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(HolziError::PassRefused {
            code: "not_a_pass_file".to_owned(),
            position: None,
            reason: Some(pkpass::Refusal::TooLarge.as_text().to_owned()),
        });
    }
    let language = args.language.unwrap_or_default();
    let parsed = tauri::async_runtime::spawn_blocking(move || pkpass::read(&bytes, &language))
        .await
        .map_err(|e| HolziError::InvalidInput {
            reason: format!("pass reader: {e}"),
        })??;
    service
        .pass_import_preview(&Caller::User, &state.pass_imports(), parsed)
        .await
}

/// Saves the pass behind a preview with the user's choices.
#[tauri::command]
pub async fn passwords_pass_import_save(
    state: State<'_, AppState>,
    args: PassImportSaveArgs,
) -> Result<PassImportSaved> {
    service(&state)?
        .pass_import_save(
            &Caller::User,
            &state.pass_imports(),
            args.preview_id,
            args.choices,
        )
        .await
}

/// Drops a preview the user did not save.
#[tauri::command]
pub async fn passwords_pass_import_cancel(
    state: State<'_, AppState>,
    args: PassImportCancelArgs,
) -> Result<()> {
    state.pass_imports().remove(&args.preview_id);
    Ok(())
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PassImportReportSaveArgs {
    pub reports: Vec<PassImportReport>,
    /// The choice of the save dialog.
    pub file: PickedFile,
}

/// Writes the reports of an import as text to the chosen file: titles, places and the names of what
/// was not kept, never a code.
#[tauri::command]
pub async fn passwords_pass_import_report_save(
    app: AppHandle,
    args: PassImportReportSaveArgs,
) -> Result<()> {
    let text = render_text(&args.reports);
    let unwritable = || HolziError::PasswordsImportFailed {
        reason: "unwritable".to_owned(),
    };
    tauri::async_runtime::spawn_blocking(move || picked::write(&app, &args.file, text.as_bytes()))
        .await
        .map_err(|_| unwritable())?
        .map_err(|_| unwritable())
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PassImportSignatureArgs {
    pub preview_id: String,
}

/// Checks the signature of the pass behind a preview, offline (FR-030). The result is information
/// for the dialog: it never stops the import and never makes a pass trusted. A file without a
/// signature is not checkable.
#[tauri::command]
pub async fn passwords_pass_import_signature(
    state: State<'_, AppState>,
    args: PassImportSignatureArgs,
) -> Result<SignatureResult> {
    let parsed = state
        .pass_imports()
        .get(&args.preview_id)
        .ok_or(HolziError::PasswordsNotFound)?;
    tauri::async_runtime::spawn_blocking(move || match &parsed.signed {
        Some(parts) => signature::check(parts),
        None => SignatureResult {
            state: SignatureState::NotCheckable,
            issuer: None,
            reason: Some("no_signature".to_owned()),
        },
    })
    .await
    .map_err(|e| HolziError::InvalidInput {
        reason: format!("signature check: {e}"),
    })
}

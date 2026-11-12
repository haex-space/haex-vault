//! The commands of passes for the window (spec 092, `contracts/tauri-commands.md`). Each is
//! `Caller::User` and calls only the service. A code value leaves only through
//! `passwords_pass_codes` and `passwords_pass_code_svg`, one pass at a time.

use serde::Deserialize;
use tauri::State;
use ts_rs::TS;

use super::service;
use crate::error::Result;
use crate::passwords::access::Caller;
use crate::passwords::model::ItemDetail;
use crate::passwords::pass::codes::{check, CodeFormat, Reason};
use crate::passwords::pass::model::{CodeCheck, CodeSvg, CodeView, PassInput, PassPatch};
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct PassCreateArgs {
    pub input: PassInput,
    pub group_id: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PassUpdateArgs {
    pub id: String,
    /// The `updatedAt` the window read; a newer one on disk is a conflict.
    pub updated_at: String,
    pub patch: PassPatch,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PassIdArgs {
    pub id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PassCodeArgs {
    pub id: String,
    pub code_id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PassCodeCheckArgs {
    pub format: String,
    pub value: String,
}

/// Creates a pass; a code typed by hand that does not fit its format is refused with its position
/// and the reason.
#[tauri::command]
pub async fn passwords_pass_create(
    state: State<'_, AppState>,
    args: PassCreateArgs,
) -> Result<ItemDetail> {
    service(&state)?
        .pass_create(&Caller::User, args.input, args.group_id)
        .await
}

/// A partial update of a pass with the conflict check.
#[tauri::command]
pub async fn passwords_pass_update(
    state: State<'_, AppState>,
    args: PassUpdateArgs,
) -> Result<ItemDetail> {
    service(&state)?
        .pass_update(&Caller::User, args.id, args.updated_at, args.patch)
        .await
}

/// The codes of one pass with their values, for the user's own screen.
#[tauri::command]
pub async fn passwords_pass_codes(
    state: State<'_, AppState>,
    args: PassIdArgs,
) -> Result<Vec<CodeView>> {
    service(&state)?.pass_codes(&Caller::User, args.id).await
}

/// The drawing of one code, dark on white with its quiet zone.
#[tauri::command]
pub async fn passwords_pass_code_svg(
    state: State<'_, AppState>,
    args: PassCodeArgs,
) -> Result<CodeSvg> {
    service(&state)?
        .pass_code_svg(&Caller::User, args.id, args.code_id)
        .await
}

/// Whether a value fits a format: the check the editor runs while typing (one rule, research R3).
/// It touches no data.
#[tauri::command]
pub async fn passwords_pass_code_check(args: PassCodeCheckArgs) -> Result<CodeCheck> {
    let reason = match CodeFormat::parse(&args.format) {
        None => Some(Reason::UnknownFormat),
        Some(format) => check(format, &args.value).err(),
    };
    Ok(CodeCheck {
        ok: reason.is_none(),
        reason: reason.map(|r| r.as_text().to_owned()),
    })
}

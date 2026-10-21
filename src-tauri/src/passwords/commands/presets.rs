//! Generator preset commands (spec 034, US3, `contracts/tauri-commands.md` §Generator). The
//! generating itself runs in the window (research R10).

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use super::service;
use crate::error::Result;
use crate::passwords::access::Caller;
use crate::passwords::model::{Preset, PresetInput};
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PresetSaveArgs {
    pub preset: PresetInput,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PresetSaveResult {
    pub preset_id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct PresetDeleteArgs {
    pub preset_id: String,
}

#[tauri::command]
pub async fn passwords_preset_list(state: State<'_, AppState>) -> Result<Vec<Preset>> {
    service(&state)?.preset_list(&Caller::User, &[]).await
}

#[tauri::command]
pub async fn passwords_preset_save(
    state: State<'_, AppState>,
    args: PresetSaveArgs,
) -> Result<PresetSaveResult> {
    let preset_id = service(&state)?
        .preset_save(&Caller::User, args.preset)
        .await?;
    Ok(PresetSaveResult { preset_id })
}

#[tauri::command]
pub async fn passwords_preset_delete(
    state: State<'_, AppState>,
    args: PresetDeleteArgs,
) -> Result<()> {
    service(&state)?
        .preset_delete(&Caller::User, args.preset_id)
        .await
}

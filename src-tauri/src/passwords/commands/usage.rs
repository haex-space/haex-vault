//! The warning before a delete (spec 034, FR-034): which holzi functions use an entry.

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use super::service;
use crate::error::Result;
use crate::passwords::access::Caller;
use crate::state::AppState;

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ItemUsageArgs {
    pub item_id: String,
}

/// The names of the functions that use the entry; empty when none is registered.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ItemUsageResult {
    pub features: Vec<String>,
}

#[tauri::command]
pub async fn passwords_item_usage(
    state: State<'_, AppState>,
    args: ItemUsageArgs,
) -> Result<ItemUsageResult> {
    // A provider may read the vault (spec 038 `StorageUsage`), so it runs off the async thread.
    let service = service(&state)?;
    let features = tauri::async_runtime::spawn_blocking(move || {
        service.item_usage(&Caller::User, &args.item_id)
    })
    .await
    .map_err(|e| crate::error::HaexError::Io {
        reason: e.to_string(),
    })??;
    Ok(ItemUsageResult { features })
}

//! Tauri commands of the extension host for holzi's own window (contracts/tauri-commands.md). An
//! extension never reaches these; it only reaches `extension_bridge_call`.

pub mod dev;
pub mod frames;
pub mod install;
pub mod manage;
pub mod model_grants;
pub mod peers;
mod permission_list;
pub mod permissions;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

/// Event `extensions-changed`: reload the extension list (launcher, settings, app list of the wm).
pub const EXTENSIONS_CHANGED: &str = "extensions-changed";

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionsChanged {
    pub extension_ids: Vec<String>,
}

pub(crate) fn emit_changed(app: &AppHandle, extension_ids: Vec<String>) {
    if let Err(error) = app.emit(EXTENSIONS_CHANGED, ExtensionsChanged { extension_ids }) {
        log::warn!("extensions-changed not delivered: {error}");
    }
}

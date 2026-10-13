//! A declared permission as the install dialog shows it (spec 017 US1; spec 018 adds the titles
//! of agent tools).

use serde::Serialize;
use ts_rs::TS;

use crate::extensions::bundle::Manifest;
use crate::extensions::permissions::manifest_map::DeclaredPermission;
use crate::extensions::permissions::PermissionKind;

/// One declared permission as the install dialog shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DeclaredPermissionView {
    pub kind: String,
    pub action: String,
    pub target: String,
    /// Remembered only for this device (shell); every other kind holds on every own device.
    pub device_scoped: bool,
    /// For `agentTool`: the tool's title per locale, from the manifest.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_de: Option<String>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_en: Option<String>,
}

impl DeclaredPermissionView {
    /// Builds an install preview row, adding localized titles for a matching manifest tool.
    pub fn of(d: &DeclaredPermission, manifest: &Manifest) -> Self {
        let tool = (d.kind == PermissionKind::AgentTool)
            .then(|| manifest.tools.iter().find(|t| t.name == d.target_text))
            .flatten();
        Self {
            kind: d.kind.as_str().to_owned(),
            action: d.action.as_string(),
            target: d.target_text.clone(),
            device_scoped: d.kind.is_device_scoped(),
            title_de: tool.map(|t| t.title_de.clone()),
            title_en: tool.map(|t| t.title_en.clone()),
        }
    }
}

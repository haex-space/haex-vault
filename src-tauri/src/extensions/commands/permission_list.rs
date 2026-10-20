//! The permissions of one extension as the settings list them (US3, T067, T070): remembered rows of
//! every device and the decisions held in memory, each with the names its target stands for (an
//! encrypted folder, an agent tool, the provider of a consent, spec 055 T049).

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::error::Result;
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::prompts::FolderNames;
use crate::extensions::permissions::store as permission_store;
use crate::extensions::permissions::{FolderRef, Permission, PermissionKind, VAULT_WIDE};
use crate::model_jobs::destinations::{self, ExtensionModelDestination};
use crate::storage::known_devices;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// One permission as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PermissionView {
    /// Row id; `None` for a decision held in memory.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub kind: String,
    pub action: String,
    pub target: String,
    /// `granted`, `denied` or `ask`.
    pub status: String,
    pub declared: bool,
    /// Holds on every own device.
    pub all_devices: bool,
    /// The device a device-scoped row holds on.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// Held only until holzi is closed.
    pub temporary: bool,
    /// holzi cannot read the row (FR-022); it counts as absent.
    pub unreadable: bool,
    /// For `encryptedFolder`: the storage's name and the folder's decrypted name as this device
    /// last read it (spec 048 FR-038); absent when this device does not know them.
    // Inline: the scripts' type check reads this binding without its siblings.
    #[ts(optional, inline)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<FolderNames>,
    /// For `agentTool` (spec 018): the tool's title per locale and whether holzi can offer it,
    /// from the manifest of the effective bundle.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_de: Option<String>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_en: Option<String>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported: Option<bool>,
    /// For `modelDestination` (spec 055): the provider the consent sends to, with its host and
    /// place; absent when the provider is gone.
    #[ts(optional, inline)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<ExtensionModelDestination>,
}

/// The names of the encrypted folder `target` (`<storage_id>/<folder_id>`), if known here.
fn folder_names(
    q: &mut impl Query,
    kind: &str,
    target: &str,
) -> haex_crdt::Result<Option<FolderNames>> {
    if kind != PermissionKind::EncryptedFolder.as_str() {
        return Ok(None);
    }
    let Some(folder) = FolderRef::parse(target) else {
        return Ok(None);
    };
    let on_device = crate::files::access::is_device_storage(&folder.storage_id);
    let storage_name = if on_device {
        Some(crate::extensions::encrypted_folder::DEVICE_NAME.to_owned())
    } else {
        crate::remote_storage::store::storage(q, &folder.storage_id)
            .map_err(|e| haex_crdt::Error::consumer(e.to_string()))?
            .map(|row| row.name)
    };
    let folder_name =
        crate::files::encrypted::cache::heads_of_folder(q, &folder.storage_id, &folder.folder_id)?
            .into_iter()
            .find_map(|(_, name)| name);
    Ok(match (storage_name, folder_name) {
        (Some(storage_name), Some(folder_name)) => Some(FolderNames {
            storage_name,
            folder_name,
            on_device,
        }),
        _ => None,
    })
}

/// The provider a `modelDestination` target names.
fn destination_of(
    q: &mut impl Query,
    kind: &str,
    target: &str,
) -> haex_crdt::Result<Option<ExtensionModelDestination>> {
    if kind != PermissionKind::ModelDestination.as_str() {
        return Ok(None);
    }
    destinations::destination(q, target)
}

/// Remembered rows of every device and the decisions held in memory.
pub fn list(db: &VaultDb, host: &ExtensionHost, extension_id: Uuid) -> Result<Vec<PermissionView>> {
    let held = host.permissions.held(extension_id);
    let held_targets: Vec<(String, String)> = held
        .iter()
        .map(|d| (d.kind.as_str().to_owned(), d.target.clone()))
        .collect();
    let (rows, devices, names, tools) = db.read_blocking(move |q| {
        let rows = permission_store::rows_of(q, extension_id)
            .map_err(|e| haex_crdt::Error::consumer(e.to_string()))?;
        let tools = crate::extensions::registry::list::effective_manifest(q, extension_id)
            .map_err(|e| haex_crdt::Error::consumer(e.to_string()))?
            .map(|(_, manifest)| manifest.tools)
            .unwrap_or_default();
        let mut names = Vec::new();
        for (kind, target) in rows
            .iter()
            .map(|r| (r.kind.clone(), r.target.clone()))
            .chain(held_targets)
        {
            names.push((
                folder_names(q, &kind, &target)?,
                destination_of(q, &kind, &target)?,
            ));
        }
        Ok((rows, known_devices::list_devices(q)?, names, tools))
    })?;
    let tool = |kind: &str, target: &str| {
        tools
            .iter()
            .find(|t| kind == PermissionKind::AgentTool.as_str() && t.name == target)
    };
    let mut names = names.into_iter();
    let device_name = |uuid: Uuid| {
        devices
            .iter()
            .find(|d| d.vault_device_uuid == uuid)
            .map(|d| d.alias.clone().unwrap_or_default())
    };
    let mut views: Vec<PermissionView> = rows
        .into_iter()
        .map(|row| {
            let (folder, destination) = names.next().unwrap_or_default();
            PermissionView {
                title_de: tool(&row.kind, &row.target).map(|t| t.title_de.clone()),
                title_en: tool(&row.kind, &row.target).map(|t| t.title_en.clone()),
                supported: tool(&row.kind, &row.target).map(|t| t.supported),
                unreadable: Permission::from_row(
                    &row.kind,
                    &row.action,
                    &row.target,
                    &row.status,
                    row.vault_device_uuid,
                )
                .is_none(),
                id: Some(row.id.to_string()),
                all_devices: row.vault_device_uuid == VAULT_WIDE,
                device_name: (row.vault_device_uuid != VAULT_WIDE)
                    .then(|| device_name(row.vault_device_uuid))
                    .flatten(),
                kind: row.kind,
                action: row.action,
                target: row.target,
                status: row.status,
                declared: row.declared,
                temporary: false,
                folder,
                destination,
            }
        })
        .collect();
    views.extend(held.into_iter().map(|held| {
        let (folder, destination) = names.next().unwrap_or_default();
        PermissionView {
            id: None,
            kind: held.kind.as_str().to_owned(),
            action: held.action,
            target: held.target,
            status: held.status.as_str().to_owned(),
            declared: false,
            all_devices: false,
            device_name: None,
            temporary: true,
            unreadable: false,
            folder,
            title_de: None,
            title_en: None,
            supported: None,
            destination,
        }
    }));
    Ok(views)
}

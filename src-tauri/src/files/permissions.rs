//! The stored file permissions of agents (spec 044 FR-031 to FR-031b, data-model.md
//! `agent_file_permissions`): rows read and written through the CRDT write path, turned into the
//! [`AgentGrants`] that [`crate::files::access::check`] evaluates. No row is a decision too: the
//! built-in agent reaches the device, nobody reaches a storage.

use haex_crdt::rusqlite::{params, Row};
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::files::access::{AgentGrants, DeviceGrant, FolderGrant, Locality, StorageGrant};
use crate::storage::query::Query;

/// `agent_id` of the agent of the holzi chat; external agents (spec 021) use their own id.
pub const BUILTIN_AGENT: &str = "builtin";

/// Namespace of the row ids: `UUIDv5(NS, agent "\n" kind "\n" target)`.
const NS_AGENT_FILES: Uuid = Uuid::from_u128(0x7c1e_52a4_6f0b_4d39_9a5e_2b8c_d14f_0e63);

const COLUMNS: &str = "agent_id, kind, target, status, updated_at, reach";

/// What a permission is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum AgentFileKind {
    /// The files of this device.
    Device,
    /// One storage of spec 038.
    Storage,
    /// One encrypted folder of a storage (spec 048), `target` = `<storage_id>/<folder_id>`.
    EncryptedFolder,
}

impl AgentFileKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Device => "device",
            Self::Storage => "storage",
            Self::EncryptedFolder => "encryptedFolder",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "device" => Some(Self::Device),
            "storage" => Some(Self::Storage),
            "encryptedFolder" => Some(Self::EncryptedFolder),
            _ => None,
        }
    }
}

/// The decision; `granted` for the device, `read` and `readWrite` for a storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum AgentFileStatus {
    Granted,
    Read,
    ReadWrite,
    Denied,
}

impl AgentFileStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Read => "read",
            Self::ReadWrite => "readWrite",
            Self::Denied => "denied",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "granted" => Some(Self::Granted),
            "read" => Some(Self::Read),
            "readWrite" => Some(Self::ReadWrite),
            "denied" => Some(Self::Denied),
            _ => None,
        }
    }

    /// Whether this status belongs to `kind`.
    fn fits(self, kind: AgentFileKind) -> bool {
        match kind {
            AgentFileKind::Device => matches!(self, Self::Granted | Self::Denied),
            AgentFileKind::Storage | AgentFileKind::EncryptedFolder => {
                matches!(self, Self::Read | Self::ReadWrite | Self::Denied)
            }
        }
    }
}

/// Which models a folder permission covers (spec 048 FR-030a): `local` only models on this device,
/// `cloud` also cloud models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum AgentFileReach {
    Local,
    Cloud,
}

impl AgentFileReach {
    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cloud => "cloud",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "local" => Some(Self::Local),
            "cloud" => Some(Self::Cloud),
            _ => None,
        }
    }
}

/// One row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentFilePermission {
    pub agent_id: String,
    pub kind: AgentFileKind,
    /// Empty for the device, the storage id for a storage.
    pub target: String,
    pub status: AgentFileStatus,
    #[ts(type = "number")]
    pub updated_at: i64,
    /// Only for an encrypted folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reach: Option<AgentFileReach>,
}

/// The target of the permission for the encrypted folder `folder_id` of `storage_id`.
pub fn folder_target(storage_id: &str, folder_id: &str) -> String {
    format!("{storage_id}/{folder_id}")
}

/// The id of the row for `agent_id`, `kind` and `target`: the same on every device.
pub fn permission_id(agent_id: &str, kind: AgentFileKind, target: &str) -> String {
    Uuid::new_v5(
        &NS_AGENT_FILES,
        format!("{agent_id}\n{}\n{target}", kind.as_str()).as_bytes(),
    )
    .to_string()
}

/// A row with a kind, status or reach holzi does not know (written by a newer version) is
/// skipped; a folder row without a reach covers only local models.
fn row_of(r: &Row<'_>) -> haex_crdt::rusqlite::Result<Option<AgentFilePermission>> {
    let kind = AgentFileKind::parse(&r.get::<_, String>(1)?);
    let status = AgentFileStatus::parse(&r.get::<_, String>(3)?);
    let reach = match (kind, r.get::<_, Option<String>>(5)?) {
        (Some(AgentFileKind::EncryptedFolder), None) => Some(AgentFileReach::Local),
        (Some(AgentFileKind::EncryptedFolder), Some(text)) => match AgentFileReach::parse(&text) {
            Some(reach) => Some(reach),
            None => return Ok(None),
        },
        _ => None,
    };
    Ok(match (kind, status) {
        (Some(kind), Some(status)) => Some(AgentFilePermission {
            agent_id: r.get(0)?,
            kind,
            target: r.get(2)?,
            status,
            updated_at: r.get(4)?,
            reach,
        }),
        _ => None,
    })
}

fn storage_grant(status: AgentFileStatus) -> StorageGrant {
    match status {
        AgentFileStatus::Read => StorageGrant::Read,
        AgentFileStatus::ReadWrite => StorageGrant::ReadWrite,
        _ => StorageGrant::Denied,
    }
}

/// Every row, by agent, kind and target.
pub fn list(q: &mut impl Query) -> Result<Vec<AgentFilePermission>> {
    Ok(q.query_map(
        &format!("SELECT {COLUMNS} FROM agent_file_permissions ORDER BY agent_id, kind, target"),
        &[],
        row_of,
    )?
    .into_iter()
    .flatten()
    .collect())
}

/// The grants of `agent_id` as [`crate::files::access::check`] reads them.
pub fn grants_of(q: &mut impl Query, agent_id: &str) -> Result<AgentGrants> {
    let rows = q.query_map(
        &format!("SELECT {COLUMNS} FROM agent_file_permissions WHERE agent_id = ?1"),
        &[&agent_id],
        row_of,
    )?;
    let mut grants = AgentGrants::default();
    for row in rows.into_iter().flatten() {
        match (row.kind, row.status) {
            (AgentFileKind::Device, AgentFileStatus::Granted) => {
                grants.device = Some(DeviceGrant::Granted);
            }
            (AgentFileKind::Device, AgentFileStatus::Denied) => {
                grants.device = Some(DeviceGrant::Denied);
            }
            (AgentFileKind::Storage, status) => {
                grants.storages.insert(row.target, storage_grant(status));
            }
            (AgentFileKind::EncryptedFolder, status) => {
                let reach = match row.reach {
                    Some(AgentFileReach::Cloud) => Locality::Cloud,
                    _ => Locality::Local,
                };
                grants.folders.insert(
                    row.target,
                    FolderGrant {
                        grant: storage_grant(status),
                        reach,
                    },
                );
            }
            _ => {}
        }
    }
    Ok(grants)
}

/// Writes `row` under its derived id: a new one is inserted, a known one updated.
pub fn set(tx: &mut CrdtTransaction<'_>, row: &AgentFilePermission) -> Result<()> {
    let target_ok = match row.kind {
        AgentFileKind::Device => row.target.is_empty() && row.reach.is_none(),
        AgentFileKind::Storage => !row.target.is_empty() && row.reach.is_none(),
        AgentFileKind::EncryptedFolder => {
            row.reach.is_some()
                && row
                    .target
                    .rsplit_once('/')
                    .is_some_and(|(storage, folder)| !storage.is_empty() && !folder.is_empty())
        }
    };
    if row.agent_id.is_empty() || !target_ok || !row.status.fits(row.kind) {
        return Err(HolziError::InvalidInput {
            reason: "not a file permission".to_owned(),
        });
    }
    let id = permission_id(&row.agent_id, row.kind, &row.target);
    let exists = tx
        .query_row(
            "SELECT 1 FROM agent_file_permissions WHERE id = ?1",
            &[&id],
            |r| r.get::<_, i64>(0),
        )?
        .is_some();
    let values = params![
        id,
        row.agent_id,
        row.kind.as_str(),
        row.target,
        row.status.as_str(),
        row.updated_at,
        row.reach.map(AgentFileReach::as_str),
    ];
    if exists {
        tx.execute(
            "UPDATE agent_file_permissions SET status = ?5, updated_at = ?6, reach = ?7 WHERE id = ?1",
            values,
        )?;
    } else {
        tx.execute(
            &format!(
                "INSERT INTO agent_file_permissions (id, {COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
            ),
            values,
        )?;
    }
    Ok(())
}

/// Removes the row of `agent_id`, `kind` and `target`: the default applies again (the device for
/// the built-in agent, a question for a storage).
pub fn remove(
    tx: &mut CrdtTransaction<'_>,
    agent_id: &str,
    kind: AgentFileKind,
    target: &str,
) -> Result<()> {
    tx.execute(
        "DELETE FROM agent_file_permissions WHERE id = ?1",
        params![permission_id(agent_id, kind, target)],
    )?;
    Ok(())
}

/// Removes every agent's permission for the storage `storage_id` and its encrypted folders; part
/// of removing the storage.
pub fn remove_storage(tx: &mut CrdtTransaction<'_>, storage_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM agent_file_permissions WHERE kind = 'storage' AND target = ?1",
        params![storage_id],
    )?;
    tx.execute(
        "DELETE FROM agent_file_permissions WHERE kind = 'encryptedFolder' \
         AND substr(target, 1, length(?1) + 1) = ?1 || '/'",
        params![storage_id],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "permissions_tests.rs"]
mod tests;

//! The words of the questions agents ask about files and the user's answers (spec 044 FR-031 to
//! FR-031b). What an agent may do is the group `files` of its agent profile since spec 083 FR-033
//! ([`crate::mcp_server::grants::FilesRights`]); nothing is stored per agent any more.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `agent_id` of the agent of the holzi chat; registered agents (spec 021) use their own id.
pub const BUILTIN_AGENT: &str = "builtin";

/// The decision; `granted` for the device and for managing file shares and sync rules, `read` and
/// `readWrite` for a storage and a folder (the contacts moved into the agent profile, spec 083).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum AgentFileStatus {
    Granted,
    Read,
    ReadWrite,
    Denied,
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

/// The target of the right for the encrypted folder `folder_id` of `storage_id`.
pub fn folder_target(storage_id: &str, folder_id: &str) -> String {
    format!("{storage_id}/{folder_id}")
}

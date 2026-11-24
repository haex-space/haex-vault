//! The rights group `memory` of an agent profile (spec 097 FR-005, research R3): its levels and the
//! scopes it opens. Re-exported by `grants`, where `Rights` holds it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The scope of the memory's reading actions (spec 097: `memory.search`, `memory.open`).
pub const MEMORY_READ_SCOPE: &str = "memory.read";
/// The scope of the memory's writing actions (`memory.add`, `memory.write`, `memory.remove`).
pub const MEMORY_WRITE_SCOPE: &str = "memory.write";

/// The rights group `memory` (spec 097 FR-005): what a model reaches of the agents' shared memory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum MemoryLevel {
    #[default]
    Off,
    Read,
    ReadWrite,
}

impl MemoryLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Read => "read",
            Self::ReadWrite => "read_write",
        }
    }

    /// The stored level; a level this build cannot read is [`Self::Off`] (FR-005).
    pub fn parse(value: &str) -> Self {
        match value {
            "read" => Self::Read,
            "read_write" => Self::ReadWrite,
            _ => Self::Off,
        }
    }

    pub fn is_off(&self) -> bool {
        *self == Self::Off
    }
}

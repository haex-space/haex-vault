//! The agent log (spec 021 FR-028, data-model.md, research R10): one row per call of an external
//! agent in `haex_mcp_agent_log_no_sync`, refused calls included. It stays on this device, never
//! holds a result, a file's content or a secret (the inputs come from [`super::redact`]), and is
//! cut to 30 days and 10 000 entries per agent whenever it is written or read.

use haex_crdt::rusqlite::params;
use haex_crdt::{CrdtTransaction, Result};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::chat::tools::action_tool::ActionEffect;

/// Entries older than this go (FR-028).
pub const MAX_AGE_MILLIS: i64 = 30 * 24 * 60 * 60 * 1000;

/// At most this many entries per agent and device.
pub const MAX_ENTRIES: i64 = 10_000;

/// The most entries one read returns (contracts/tauri-commands.md).
pub const MAX_PAGE: u32 = 200;

/// How a call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum Outcome {
    Ok,
    DeniedRights,
    DeniedUser,
    DeniedPlan,
    NoAnswer,
    Error,
    Unavailable,
    TooMany,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::DeniedRights => "denied_rights",
            Self::DeniedUser => "denied_user",
            Self::DeniedPlan => "denied_plan",
            Self::NoAnswer => "no_answer",
            Self::Error => "error",
            Self::Unavailable => "unavailable",
            Self::TooMany => "too_many",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [
            Self::Ok,
            Self::DeniedRights,
            Self::DeniedUser,
            Self::DeniedPlan,
            Self::NoAnswer,
            Self::Error,
            Self::Unavailable,
            Self::TooMany,
        ]
        .into_iter()
        .find(|outcome| outcome.as_str() == value)
    }
}

/// The name of an effect in the `effect` column; empty for a tool holzi does not know.
pub fn effect_name(effect: Option<ActionEffect>) -> &'static str {
    match effect {
        Some(ActionEffect::Read) => "read",
        Some(ActionEffect::Write) => "write",
        Some(ActionEffect::Destructive) => "destructive",
        None => "",
    }
}

/// What one call writes.
#[derive(Debug, Clone)]
pub struct NewEntry {
    pub tool: String,
    pub effect: Option<ActionEffect>,
    pub target: Option<String>,
    /// The redacted inputs as JSON.
    pub input: Option<String>,
    pub outcome: Outcome,
    /// A call prompt was shown.
    pub asked: bool,
}

/// One entry as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentLogEntry {
    pub id: String,
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub created_at: i64,
    pub tool: String,
    /// `read`, `write`, `destructive`, or empty for a tool holzi does not know.
    pub effect: String,
    pub target: Option<String>,
    /// The redacted inputs as JSON.
    pub input: Option<String>,
    pub outcome: Outcome,
    pub asked: bool,
}

/// Writes `entry` for `agent` and cuts the log.
pub fn write(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    agent: Uuid,
    entry: &NewEntry,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO haex_mcp_agent_log_no_sync (id, vault_device_uuid, agent_id, created_at, \
         tool, effect, target, input, outcome, asked) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            Uuid::new_v4().to_string(),
            device.to_string(),
            agent.to_string(),
            now,
            entry.tool,
            effect_name(entry.effect),
            entry.target,
            entry.input,
            entry.outcome.as_str(),
            i64::from(entry.asked),
        ],
    )?;
    prune(tx, device, agent, now)
}

/// Removes the entries of `agent` older than 30 days and beyond the newest 10 000.
pub fn prune(tx: &mut CrdtTransaction<'_>, device: Uuid, agent: Uuid, now: i64) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_mcp_agent_log_no_sync \
         WHERE vault_device_uuid = ?1 AND agent_id = ?2 AND created_at < ?3",
        params![device.to_string(), agent.to_string(), now - MAX_AGE_MILLIS],
    )?;
    tx.execute(
        "DELETE FROM haex_mcp_agent_log_no_sync \
         WHERE vault_device_uuid = ?1 AND agent_id = ?2 AND id NOT IN ( \
           SELECT id FROM haex_mcp_agent_log_no_sync \
           WHERE vault_device_uuid = ?1 AND agent_id = ?2 \
           ORDER BY created_at DESC, rowid DESC LIMIT ?3)",
        params![device.to_string(), agent.to_string(), MAX_ENTRIES],
    )?;
    Ok(())
}

/// The entries of `agent` before `(created_at, id)` (all when `before` is `None`), newest first,
/// at most `limit` (capped at [`MAX_PAGE`]). The id breaks ties when calls share a millisecond.
/// Cuts the log first, so an old entry is gone even without a new call.
pub fn read(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    agent: Uuid,
    before: Option<i64>,
    before_id: Option<&str>,
    limit: u32,
    now: i64,
) -> Result<Vec<AgentLogEntry>> {
    prune(tx, device, agent, now)?;
    let rows = tx.query_map(
        "SELECT id, created_at, tool, effect, target, input, outcome, asked \
         FROM haex_mcp_agent_log_no_sync \
         WHERE vault_device_uuid = ?1 AND agent_id = ?2 \
           AND (created_at < ?3 OR (created_at = ?3 AND ?4 IS NOT NULL AND id < ?4)) \
         ORDER BY created_at DESC, id DESC LIMIT ?5",
        params![
            device.to_string(),
            agent.to_string(),
            before.unwrap_or(i64::MAX),
            before_id,
            limit.min(MAX_PAGE),
        ],
        |row| {
            let outcome: String = row.get(6)?;
            // A row a newer build wrote with an outcome this one does not know is left out.
            let Some(outcome) = Outcome::parse(&outcome) else {
                return Ok(None);
            };
            Ok(Some(AgentLogEntry {
                id: row.get(0)?,
                created_at: row.get(1)?,
                tool: row.get(2)?,
                effect: row.get(3)?,
                target: row.get(4)?,
                input: row.get(5)?,
                outcome,
                asked: row.get::<_, i64>(7)? != 0,
            }))
        },
    )?;
    Ok(rows.into_iter().flatten().collect())
}

/// Empties the log of `agent` (FR-028, "Protokoll leeren").
pub fn clear(tx: &mut CrdtTransaction<'_>, device: Uuid, agent: Uuid) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_mcp_agent_log_no_sync WHERE vault_device_uuid = ?1 AND agent_id = ?2",
        params![device.to_string(), agent.to_string()],
    )?;
    Ok(())
}

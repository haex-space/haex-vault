//! The external agents registered on this device and their rights (spec 021, data-model.md): rows of
//! `haex_mcp_agents` and `haex_mcp_agent_grants` for this device only (ADR-0001). An agent is found
//! by its credential, which holzi keeps only as a SHA-256 hash (FR-007); the name it reports is its
//! own claim and only shown.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use haex_crdt::rusqlite::params;
use haex_crdt::{CrdtTransaction, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use uuid::Uuid;

use super::grant_rows::{rights_of, rows_of, GrantRow};
use super::grants::Rights;
use crate::chat::tools::permission::PermissionMode;
use crate::storage::query::Query;

/// The longest reported name holzi keeps (data-model.md).
pub const MAX_CLIENT_NAME_CHARS: usize = 64;

/// Namespace of the row ids of `haex_mcp_agent_grants`.
const NS_MCP_GRANT: Uuid = Uuid::from_u128(0x6d63_7021_4a3e_4b8c_9f12_0e7d_5a21_c021);

/// What kind of agent a row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum AgentKind {
    /// Any MCP client that started the helper or connected over HTTP.
    External,
    ClaudeCodeDelegate,
    CodexDelegate,
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::External => "external",
            Self::ClaudeCodeDelegate => "claude_code_delegate",
            Self::CodexDelegate => "codex_delegate",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "external" => Some(Self::External),
            "claude_code_delegate" => Some(Self::ClaudeCodeDelegate),
            "codex_delegate" => Some(Self::CodexDelegate),
            _ => None,
        }
    }
}

/// How the agent reaches holzi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum Transport {
    /// The `--mcp-agent` helper over a Unix socket or a named pipe.
    Ipc,
    Http,
}

impl Transport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ipc => "ipc",
            Self::Http => "http",
        }
    }

    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "ipc" => Some(Self::Ipc),
            "http" => Some(Self::Http),
            _ => None,
        }
    }
}

/// The state of a row (data-model.md, "Zustände").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum AgentStatus {
    Registered,
    Expired,
    Blocked,
}

impl AgentStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::Expired => "expired",
            Self::Blocked => "blocked",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "registered" => Some(Self::Registered),
            "expired" => Some(Self::Expired),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }
}

/// The approval mode of one agent (FR-021); only the user changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum AgentMode {
    #[default]
    Manual,
    Auto,
    Plan,
}

impl AgentMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
            Self::Plan => "plan",
        }
    }

    /// The gate's mode (`chat::tools::permission`).
    pub fn permission_mode(self) -> PermissionMode {
        match self {
            Self::Manual => PermissionMode::Manual,
            Self::Auto => PermissionMode::Auto,
            Self::Plan => PermissionMode::Plan,
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match PermissionMode::parse(value)? {
            PermissionMode::Manual => Some(Self::Manual),
            PermissionMode::Auto => Some(Self::Auto),
            PermissionMode::Plan => Some(Self::Plan),
        }
    }
}

/// A row of `haex_mcp_agents`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub id: Uuid,
    pub kind: AgentKind,
    pub slug: String,
    /// The agent's own claim, cut to [`MAX_CLIENT_NAME_CHARS`].
    pub client_name: String,
    pub client_version: Option<String>,
    pub transport: Transport,
    pub status: AgentStatus,
    pub mode: AgentMode,
    pub registered_at: i64,
    pub last_seen_at: Option<i64>,
}

/// A fresh credential. Shown to the agent once; holzi keeps only [`hash`] of it.
pub struct Credential(String);

impl Credential {
    /// 32 random bytes, base64url without padding.
    pub fn generate() -> std::io::Result<Self> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|error| std::io::Error::other(error.to_string()))?;
        Ok(Self(URL_SAFE_NO_PAD.encode(bytes)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credential(…)")
    }
}

/// The SHA-256 of a credential, base64url; what `token_hash` holds.
pub fn hash(credential: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(credential.as_bytes()))
}

/// Cuts a reported name to the kept length.
pub fn kept_name(name: &str) -> String {
    name.trim().chars().take(MAX_CLIENT_NAME_CHARS).collect()
}

const COLUMNS: &str = "id, kind, slug, client_name, client_version, transport, status, \
                       permission_mode, registered_at, last_seen_at";

fn from_row(row: &haex_crdt::rusqlite::Row<'_>) -> haex_crdt::rusqlite::Result<Option<Agent>> {
    let id: String = row.get(0)?;
    let kind: String = row.get(1)?;
    let transport: String = row.get(5)?;
    let status: String = row.get(6)?;
    let mode: String = row.get(7)?;
    // A row another device or a newer build wrote in a shape this one cannot read counts as absent.
    let (Ok(id), Some(kind), Some(transport), Some(status), Some(mode)) = (
        Uuid::parse_str(&id),
        AgentKind::parse(&kind),
        Transport::parse(&transport),
        AgentStatus::parse(&status),
        AgentMode::parse(&mode),
    ) else {
        return Ok(None);
    };
    Ok(Some(Agent {
        id,
        kind,
        slug: row.get(2)?,
        client_name: row.get(3)?,
        client_version: row.get(4)?,
        transport,
        status,
        mode,
        registered_at: row.get(8)?,
        last_seen_at: row.get(9)?,
    }))
}

/// The agent of `device` whose credential is `credential`, in any state.
pub fn find_by_credential(
    q: &mut impl Query,
    device: Uuid,
    credential: &str,
) -> Result<Option<Agent>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM haex_mcp_agents WHERE vault_device_uuid = ?1 AND token_hash = ?2"
    );
    Ok(q.query_row(
        &sql,
        params![device.to_string(), hash(credential)],
        from_row,
    )?
    .flatten())
}

/// Whether the user blocked the agent of `slug` on `device` through `transport`.
pub fn is_blocked(
    q: &mut impl Query,
    device: Uuid,
    slug: &str,
    transport: Transport,
) -> Result<bool> {
    Ok(q.query_row(
        "SELECT COUNT(*) FROM haex_mcp_agents WHERE vault_device_uuid = ?1 AND slug = ?2 \
         AND transport = ?3 AND status = 'blocked'",
        params![device.to_string(), slug, transport.as_str()],
        |row| row.get::<_, i64>(0),
    )?
    .unwrap_or(0)
        > 0)
}

/// The agent `id` of `device`, in any state.
pub fn get(q: &mut impl Query, device: Uuid, id: Uuid) -> Result<Option<Agent>> {
    let sql =
        format!("SELECT {COLUMNS} FROM haex_mcp_agents WHERE vault_device_uuid = ?1 AND id = ?2");
    Ok(
        q.query_row(&sql, params![device.to_string(), id.to_string()], from_row)?
            .flatten(),
    )
}

/// Every agent of `device`, newest first.
pub fn list(q: &mut impl Query, device: Uuid) -> Result<Vec<Agent>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM haex_mcp_agents WHERE vault_device_uuid = ?1 \
         ORDER BY registered_at DESC"
    );
    Ok(q.query_map(&sql, params![device.to_string()], from_row)?
        .into_iter()
        .flatten()
        .collect())
}

/// What a registration writes.
#[derive(Debug, Clone)]
pub struct NewAgent {
    pub kind: AgentKind,
    pub slug: String,
    pub client_name: String,
    pub client_version: Option<String>,
    pub transport: Transport,
    pub mode: AgentMode,
}

/// Writes a registered agent with its rights; returns its row.
pub fn register(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    new: &NewAgent,
    credential_hash: &str,
    rights: &Rights,
    now: i64,
) -> Result<Agent> {
    let agent = Agent {
        id: Uuid::new_v4(),
        kind: new.kind,
        slug: new.slug.clone(),
        client_name: kept_name(&new.client_name),
        client_version: new.client_version.as_deref().map(kept_name),
        transport: new.transport,
        status: AgentStatus::Registered,
        mode: new.mode,
        registered_at: now,
        last_seen_at: Some(now),
    };
    tx.execute(
        "INSERT INTO haex_mcp_agents (id, vault_device_uuid, kind, slug, client_name, \
         client_version, transport, oauth_client_id, token_hash, status, permission_mode, \
         registered_at, last_seen_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11, ?12)",
        params![
            agent.id.to_string(),
            device.to_string(),
            agent.kind.as_str(),
            agent.slug,
            agent.client_name,
            agent.client_version,
            agent.transport.as_str(),
            credential_hash,
            agent.status.as_str(),
            agent.mode.as_str(),
            agent.registered_at,
            agent.last_seen_at,
        ],
    )?;
    save_rights(tx, device, agent.id, rights)?;
    Ok(agent)
}

/// Registers the delegate `kind` (spec 021 US6, data-model.md): one row per kind and device, its
/// id from both (`delegates::delegate_id`), no credential; an existing row is renewed.
pub fn register_delegate(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    kind: AgentKind,
    client_name: &str,
    rights: &Rights,
    mode: AgentMode,
    now: i64,
) -> Result<Agent> {
    let id = super::delegates::delegate_id(device, kind);
    let slug = super::delegates::slug_of(kind).unwrap_or_default();
    let renewed = tx.execute(
        "UPDATE haex_mcp_agents SET status = 'registered', permission_mode = ?3, \
         client_name = ?4, token_hash = NULL, last_seen_at = ?5 \
         WHERE id = ?1 AND vault_device_uuid = ?2",
        params![
            id.to_string(),
            device.to_string(),
            mode.as_str(),
            client_name,
            now
        ],
    )?;
    if renewed == 0 {
        tx.execute(
            "INSERT INTO haex_mcp_agents (id, vault_device_uuid, kind, slug, client_name, \
             client_version, transport, oauth_client_id, token_hash, status, permission_mode, \
             registered_at, last_seen_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'ipc', NULL, NULL, 'registered', ?6, ?7, ?7)",
            params![
                id.to_string(),
                device.to_string(),
                kind.as_str(),
                slug,
                client_name,
                mode.as_str(),
                now,
            ],
        )?;
    }
    save_rights(tx, device, id, rights)?;
    get(tx, device, id)?.ok_or_else(|| haex_crdt::rusqlite::Error::QueryReturnedNoRows.into())
}

/// Remembers that the user blocked the agent of `slug` (data-model.md: no credential).
pub fn block(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    slug: &str,
    client_name: &str,
    transport: Transport,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO haex_mcp_agents (id, vault_device_uuid, kind, slug, client_name, \
         client_version, transport, oauth_client_id, token_hash, status, permission_mode, \
         registered_at, last_seen_at) \
         VALUES (?1, ?2, 'external', ?3, ?4, NULL, ?5, NULL, NULL, 'blocked', 'manual', ?6, NULL)",
        params![
            Uuid::new_v4().to_string(),
            device.to_string(),
            slug,
            kept_name(client_name),
            transport.as_str(),
            now,
        ],
    )?;
    Ok(())
}

/// Notes a connection of `agent`.
pub fn touch(tx: &mut CrdtTransaction<'_>, device: Uuid, agent: Uuid, now: i64) -> Result<()> {
    tx.execute(
        "UPDATE haex_mcp_agents SET last_seen_at = ?3 WHERE id = ?1 AND vault_device_uuid = ?2",
        params![agent.to_string(), device.to_string(), now],
    )?;
    Ok(())
}

fn grant_id(device: Uuid, agent: Uuid, group: &str, key: &str) -> String {
    Uuid::new_v5(
        &NS_MCP_GRANT,
        format!("{device}:{agent}:{group}:{key}").as_bytes(),
    )
    .to_string()
}

/// The rights of `agent`. A row this build cannot read counts as no right.
pub fn load_rights(q: &mut impl Query, device: Uuid, agent: Uuid) -> Result<Rights> {
    let rows = q.query_map(
        "SELECT grant_group, grant_key, level, scope_tags FROM haex_mcp_agent_grants \
         WHERE vault_device_uuid = ?1 AND agent_id = ?2",
        params![device.to_string(), agent.to_string()],
        |row| {
            Ok(GrantRow {
                group: row.get(0)?,
                key: row.get(1)?,
                level: row.get(2)?,
                tags: row.get(3)?,
            })
        },
    )?;
    Ok(rights_of(rows))
}

/// Replaces the rights of `agent`. The caller has validated them (`grants::validate`).
pub fn save_rights(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    agent: Uuid,
    rights: &Rights,
) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_mcp_agent_grants WHERE vault_device_uuid = ?1 AND agent_id = ?2",
        params![device.to_string(), agent.to_string()],
    )?;
    for row in rows_of(rights) {
        tx.execute(
            "INSERT INTO haex_mcp_agent_grants (id, vault_device_uuid, agent_id, grant_group, \
             grant_key, level, scope_tags) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                grant_id(device, agent, &row.group, &row.key),
                device.to_string(),
                agent.to_string(),
                row.group,
                row.key,
                row.level,
                row.tags,
            ],
        )?;
    }
    Ok(())
}

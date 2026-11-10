//! The external agents registered on this device (spec 021, data-model.md): rows of
//! `haex_mcp_agents` for this device only (ADR-0001). Their rights are not kept here: an agent is
//! in an agent profile (spec 083, `crate::agent_profiles`) and [`load_rights`] resolves it. An agent is found
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

use super::grants::Rights;
use crate::chat::tools::permission::PermissionMode;
use crate::storage::query::Query;

/// The longest reported name holzi keeps (data-model.md).
pub const MAX_CLIENT_NAME_CHARS: usize = 64;

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

    pub(crate) fn parse(value: &str) -> Option<Self> {
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
    /// A blocked row saved by a newer build with a transport this one cannot use. Kept visible so
    /// the user can remove the block; `parse` intentionally still refuses it for active rows.
    Unknown,
}

impl Transport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ipc => "ipc",
            Self::Http => "http",
            Self::Unknown => "unknown",
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

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "registered" => Some(Self::Registered),
            "expired" => Some(Self::Expired),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }
}

/// The approval mode of one agent (FR-021, ADR-0006). An external agent always runs in `Auto`: holzi
/// is an MCP server like any other, the agent profile decides what it may do, and holzi asks only
/// before a destructive call or one that hands out a secret. A model of an Abo-Agent follows the
/// mode of the chat.
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

    pub(crate) fn parse(value: &str) -> Option<Self> {
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
    let (Ok(id), Some(kind), transport, Some(status), Some(mode)) = (
        Uuid::parse_str(&id),
        AgentKind::parse(&kind),
        Transport::parse(&transport),
        AgentStatus::parse(&status),
        AgentMode::parse(&mode),
    ) else {
        return Ok(None);
    };
    let Some(transport) =
        transport.or_else(|| (status == AgentStatus::Blocked).then_some(Transport::Unknown))
    else {
        return Ok(None);
    };
    // The stored mode of an external agent is no setting any more: a row an earlier build wrote
    // with `manual` or `plan` runs in `Auto` like every other.
    let mode = if kind == AgentKind::External {
        AgentMode::Auto
    } else {
        mode
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
}

/// Writes a registered agent in the agent profile `profile` ("Standard" writes no assignment);
/// returns its row.
pub fn register(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    new: &NewAgent,
    credential_hash: &str,
    profile: Uuid,
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
        mode: AgentMode::Auto,
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
    crate::agent_profiles::store::add_member(
        tx,
        &crate::agent_profiles::Subject::Agent(agent.id),
        profile,
    )?;
    Ok(agent)
}

/// Registers the legacy per-kind delegate row, used when converting memberships from older vaults.
pub fn register_delegate(
    tx: &mut CrdtTransaction<'_>,
    device: Uuid,
    kind: AgentKind,
    client_name: &str,
    profile: Uuid,
    mode: AgentMode,
    now: i64,
) -> Result<Agent> {
    let id = super::delegates::delegate_id(device, kind);
    register_delegate_as(
        tx,
        DelegateRegistration {
            device,
            id,
            kind,
            client_name,
            profile,
            mode,
            now,
        },
    )
}

/// Registers the technical session identity for one Abo-Agent model.
pub(crate) fn register_delegate_for_model(
    tx: &mut CrdtTransaction<'_>,
    registration: DelegateModelRegistration<'_>,
) -> Result<Agent> {
    let id = super::delegates::model_delegate_id(
        registration.device,
        registration.kind,
        &registration.model,
    );
    register_delegate_as(
        tx,
        DelegateRegistration {
            device: registration.device,
            id,
            kind: registration.kind,
            client_name: registration.client_name,
            profile: registration.profile,
            mode: registration.mode,
            now: registration.now,
        },
    )
}

pub(crate) struct DelegateModelRegistration<'a> {
    pub(crate) device: Uuid,
    pub(crate) kind: AgentKind,
    pub(crate) model: String,
    pub(crate) client_name: &'a str,
    pub(crate) profile: Uuid,
    pub(crate) mode: AgentMode,
    pub(crate) now: i64,
}

struct DelegateRegistration<'a> {
    device: Uuid,
    id: Uuid,
    kind: AgentKind,
    client_name: &'a str,
    profile: Uuid,
    mode: AgentMode,
    now: i64,
}

fn register_delegate_as(
    tx: &mut CrdtTransaction<'_>,
    registration: DelegateRegistration<'_>,
) -> Result<Agent> {
    let DelegateRegistration {
        device,
        id,
        kind,
        client_name,
        profile,
        mode,
        now,
    } = registration;
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
    crate::agent_profiles::store::add_member(
        tx,
        &crate::agent_profiles::Subject::Agent(id),
        profile,
    )?;
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

/// Who an agent is to the agent profiles: itself, or for a delegate the model its running start
/// uses (spec 083 FR-028). A delegate with no start running has no subject but its own row, which
/// no profile names, so its rights are none.
pub fn subject_of(agent: &Agent) -> crate::agent_profiles::Subject {
    use crate::agent_profiles::Subject;
    match agent.kind {
        AgentKind::External => Subject::Agent(agent.id),
        AgentKind::ClaudeCodeDelegate | AgentKind::CodexDelegate => {
            match super::delegates::live_model(agent.id) {
                Some(model) => Subject::Model(model),
                None => Subject::Agent(agent.id),
            }
        }
    }
}

/// [`subject_of`] for the agent with this id; an unknown id is itself.
pub fn subject_of_id(
    q: &mut impl Query,
    device: Uuid,
    agent: Uuid,
) -> Result<crate::agent_profiles::Subject> {
    Ok(match get(q, device, agent)? {
        Some(found) => subject_of(&found),
        None => crate::agent_profiles::Subject::Agent(agent),
    })
}

/// The rights of `agent`: those of its agent profile, "Standard" when it has none (spec 083); for
/// a delegate those of the model it runs with. An agent that is unknown or blocked has none.
pub fn load_rights(q: &mut impl Query, device: Uuid, agent: Uuid) -> Result<Rights> {
    let Some(found) = get(q, device, agent)?.filter(|agent| agent.status != AgentStatus::Blocked)
    else {
        return Ok(Rights::default());
    };
    if found.kind != AgentKind::External && super::delegates::live_model(found.id).is_none() {
        return Ok(Rights::default());
    }
    crate::agent_profiles::resolve::rights_of(q, &subject_of(&found))
}

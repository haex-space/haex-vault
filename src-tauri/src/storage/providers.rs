//! Typed writes and reads for the `providers` table.
//!
//! Etappe 2 groundwork: this slice ships the schema and storage helpers
//! only. Live provider invocation (Anthropic/OpenAI/CLI delegate) is a
//! later slice; this module makes sure the row layout the deferred sync
//! path expects is already frozen.

use ::uuid::Uuid;
use haex_crdt::rusqlite::{params, Result};
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};

use crate::storage::query::Query;
use crate::storage::uuid;

/// Provider row as stored in SQLite. `credentials` is opaque bytes — for
/// `api_key` providers it is the API key, and for `cli_delegate`
/// providers (spec 007-cli-delegate) it is a Claude OAuth token or a
/// Codex `auth.json`'s raw bytes, depending on `adapter` (SQLCipher
/// protects at rest either way, and the deferred sync payload transports
/// it only over the encrypted channel per plan §"Anbietermodelle"). For
/// `local` providers it is `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: Uuid,
    pub kind: ProviderKind,
    /// Vendor discriminator: for `api_key` providers a vendor like
    /// `"anthropic"`; for `cli_delegate` providers (spec 007-cli-delegate)
    /// `"claude"` or `"codex"`. `None` for legacy rows and `local`.
    pub adapter: Option<String>,
    pub name: String,
    pub base_url: Option<String>,
    pub credentials: Option<Vec<u8>>,
    pub created_at: i64,
    /// What this row is used for — orthogonal to `kind` (spec 008: a
    /// `Local` row can be either the chat model or the bundled Parakeet
    /// transcription adapter). Defaults to `Chat` for every row that
    /// predates this column (migration `0016_providers_add_capability`).
    pub capability: ProviderCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Local,
    ApiKey,
    CliDelegate,
}

impl ProviderKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Local => "local",
            ProviderKind::ApiKey => "api_key",
            ProviderKind::CliDelegate => "cli_delegate",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "local" => Some(ProviderKind::Local),
            "api_key" => Some(ProviderKind::ApiKey),
            "cli_delegate" => Some(ProviderKind::CliDelegate),
            _ => None,
        }
    }
}

/// What a provider row is used for (spec 008 data-model.md
/// §"Schemaänderung: providers.capability"). Orthogonal to `ProviderKind` —
/// e.g. a `Local` row can be the chat model (`Chat`) or the bundled Parakeet
/// adapter (`Transcription`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapability {
    Chat,
    Transcription,
}

impl ProviderCapability {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProviderCapability::Chat => "chat",
            ProviderCapability::Transcription => "transcription",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "chat" => Some(ProviderCapability::Chat),
            "transcription" => Some(ProviderCapability::Transcription),
            _ => None,
        }
    }
}

/// Inserts a provider row.
pub fn insert_provider(tx: &mut CrdtTransaction<'_>, p: &Provider) -> haex_crdt::Result<usize> {
    tx.execute(
        "INSERT INTO providers \
           (id, kind, adapter, name, base_url, credentials, created_at, capability) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            p.id.to_string(),
            p.kind.as_str(),
            p.adapter,
            p.name,
            p.base_url,
            p.credentials,
            p.created_at,
            p.capability.as_str(),
        ],
    )
}

/// Replaces a provider's fields while preserving its id.
pub fn update_provider(tx: &mut CrdtTransaction<'_>, p: &Provider) -> haex_crdt::Result<usize> {
    tx.execute(
        "UPDATE providers SET kind = ?1, adapter = ?2, name = ?3, base_url = ?4, \
         credentials = ?5, created_at = ?6, capability = ?7 WHERE id = ?8",
        params![
            p.kind.as_str(),
            p.adapter,
            p.name,
            p.base_url,
            p.credentials,
            p.created_at,
            p.capability.as_str(),
            p.id.to_string(),
        ],
    )
}

/// Deletes a provider by id.
pub fn delete_provider(tx: &mut CrdtTransaction<'_>, id: Uuid) -> haex_crdt::Result<usize> {
    tx.execute(
        "DELETE FROM providers WHERE id = ?1",
        params![id.to_string()],
    )
}

/// Points every model, thread and message that uses provider `from` at
/// provider `to` instead. Used when a local provider row moves to its fixed
/// id (`providers::local`, spec 024).
pub fn move_provider_references(
    tx: &mut CrdtTransaction<'_>,
    from: Uuid,
    to: Uuid,
) -> haex_crdt::Result<()> {
    let (from, to) = (from.to_string(), to.to_string());
    for sql in [
        "UPDATE models SET provider_id = ?1 WHERE provider_id = ?2",
        "UPDATE chat_threads SET last_provider_id = ?1 WHERE last_provider_id = ?2",
        "UPDATE chat_messages SET provider_id = ?1 WHERE provider_id = ?2",
    ] {
        tx.execute(sql, params![to, from])?;
    }
    Ok(())
}

/// Ids of every row of one `(kind, capability)` pair, oldest first.
pub fn provider_ids_by_kind_and_capability(
    q: &mut impl Query,
    kind: ProviderKind,
    capability: ProviderCapability,
) -> haex_crdt::Result<Vec<Uuid>> {
    let raw: Vec<String> = q.query_map(
        "SELECT id FROM providers WHERE kind = ?1 AND capability = ?2 \
         ORDER BY created_at ASC",
        params![kind.as_str(), capability.as_str()],
        |r| r.get(0),
    )?;
    raw.into_iter()
        .map(|value| uuid::parse(&value, 0).map_err(Into::into))
        .collect()
}

/// Persists a repaired adapter discriminator and marks the CRDT row dirty.
pub fn set_adapter(
    tx: &mut CrdtTransaction<'_>,
    id: Uuid,
    adapter: &str,
) -> haex_crdt::Result<usize> {
    tx.execute(
        "UPDATE providers SET adapter = ?1 WHERE id = ?2",
        params![adapter, id.to_string()],
    )
}

/// Overwrites a provider's stored credentials in place (spec 007-cli-delegate
/// `connect_cli_delegate`/`submit_cli_delegate_code` upsert semantics,
/// contracts/tauri-commands.md — a reconnect updates the existing
/// `cli_delegate` row instead of inserting a duplicate).
pub fn update_credentials(
    tx: &mut CrdtTransaction<'_>,
    id: Uuid,
    credentials: &[u8],
) -> haex_crdt::Result<usize> {
    tx.execute(
        "UPDATE providers SET credentials = ?1 WHERE id = ?2",
        params![credentials, id.to_string()],
    )
}

/// Finds the singleton `cli_delegate` provider row for one vendor, if any
/// (mirrors `providers::local::find_local_provider`'s one-row-per-kind
/// pattern, scoped further by `adapter`).
pub fn find_cli_delegate_provider(
    q: &mut impl Query,
    vendor: &str,
) -> haex_crdt::Result<Option<Uuid>> {
    let raw: Option<String> = q.query_row(
        "SELECT id FROM providers WHERE kind = ?1 AND adapter = ?2 \
         ORDER BY created_at ASC LIMIT 1",
        params![ProviderKind::CliDelegate.as_str(), vendor],
        |r| r.get(0),
    )?;
    Ok(raw.map(|value| uuid::parse(&value, 0)).transpose()?)
}

/// Finds the singleton row for one `(kind, capability)` pair, if any —
/// shared by `providers::local::find_local_provider` (capability `chat`)
/// and `find_local_transcription_provider` (capability `transcription`).
/// Without the `capability` filter, two `kind = 'local'` rows exist once a
/// local transcription provider is created (spec 008 data-model.md), and
/// which one an unqualified `kind`-only query returns would depend on
/// `created_at` ordering.
pub fn find_provider_by_kind_and_capability(
    q: &mut impl Query,
    kind: ProviderKind,
    capability: ProviderCapability,
) -> haex_crdt::Result<Option<Uuid>> {
    let raw: Option<String> = q.query_row(
        "SELECT id FROM providers WHERE kind = ?1 AND capability = ?2 \
         ORDER BY created_at ASC LIMIT 1",
        params![kind.as_str(), capability.as_str()],
        |r| r.get(0),
    )?;
    Ok(raw.map(|value| uuid::parse(&value, 0)).transpose()?)
}

/// Lists all providers ordered by creation time (oldest first).
pub fn list_providers(q: &mut impl Query) -> haex_crdt::Result<Vec<Provider>> {
    q.query_map(
        "SELECT id, kind, adapter, name, base_url, credentials, created_at, capability \
         FROM providers ORDER BY created_at ASC",
        &[],
        row_to_provider,
    )
}

/// Fetches one provider by id.
pub fn get_provider(q: &mut impl Query, id: Uuid) -> haex_crdt::Result<Option<Provider>> {
    q.query_row(
        "SELECT id, kind, adapter, name, base_url, credentials, created_at, capability \
         FROM providers WHERE id = ?1",
        params![id.to_string()],
        row_to_provider,
    )
}

fn row_to_provider(row: &haex_crdt::rusqlite::Row<'_>) -> Result<Provider> {
    let id_str: String = row.get(0)?;
    let kind_str: String = row.get(1)?;
    let capability_str: String = row.get(7)?;
    Ok(Provider {
        id: uuid::parse(&id_str, 0)?,
        kind: ProviderKind::parse(&kind_str).ok_or_else(|| {
            haex_crdt::rusqlite::Error::FromSqlConversionFailure(
                1,
                haex_crdt::rusqlite::types::Type::Text,
                format!("unknown provider kind: {kind_str}").into(),
            )
        })?,
        adapter: row.get(2)?,
        name: row.get(3)?,
        base_url: row.get(4)?,
        credentials: row.get(5)?,
        created_at: row.get(6)?,
        capability: ProviderCapability::parse(&capability_str).ok_or_else(|| {
            haex_crdt::rusqlite::Error::FromSqlConversionFailure(
                7,
                haex_crdt::rusqlite::types::Type::Text,
                format!("unknown provider capability: {capability_str}").into(),
            )
        })?,
    })
}

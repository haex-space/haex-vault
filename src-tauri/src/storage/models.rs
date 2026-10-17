//! Typed writes and reads for the `models` table.
//!
//! `models` is the cross-provider catalog cache. Rows are inserted when
//! a provider's model list is fetched (plan §"Anbietermodelle":
//! "Modelllisten werden immer abgefragt, nie hartkodiert") and when a
//! local GGUF is downloaded / imported.

use std::collections::HashSet;

use ::uuid::Uuid;
use haex_crdt::rusqlite::{params, Result};
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};

use crate::model_capabilities::{ModelCapabilities, ToolUse, UserCapabilities};
use crate::storage::query::Query;
use crate::storage::uuid;

/// Where a `models` row's file came from. Persisted in `source_kind`
/// (migration `0015_models_add_huggingface_source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Seeded from the built-in curated catalog.
    Catalog,
    /// Installed via the free HuggingFace discovery/download flow.
    Huggingface,
    /// Copied in from an operator-picked local file.
    Imported,
    /// An `api_key` / remote provider's model — never has a local file.
    Provider,
}

impl SourceKind {
    fn as_str(self) -> &'static str {
        match self {
            SourceKind::Catalog => "catalog",
            SourceKind::Huggingface => "huggingface",
            SourceKind::Imported => "imported",
            SourceKind::Provider => "provider",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "catalog" => SourceKind::Catalog,
            "huggingface" => SourceKind::Huggingface,
            "imported" => SourceKind::Imported,
            // Unknown/legacy values fall back to `provider`, the migration
            // default — never misclassified as `catalog`/`huggingface`.
            _ => SourceKind::Provider,
        }
    }
}

/// Visible integrity state of a local model file against its stored
/// `file_sha256` (spec 005 Entscheidung 6). Persisted in
/// `integrity_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityStatus {
    /// The last hash check matched `file_sha256` exactly.
    Verified,
    /// The operator explicitly accepted a mismatch via
    /// `load_model_with_integrity_override`; `file_sha256` is unchanged.
    Untrusted,
    /// No check has run yet, or the row predates hashing
    /// (migration default for every pre-existing row).
    Unknown,
}

impl IntegrityStatus {
    fn as_str(self) -> &'static str {
        match self {
            IntegrityStatus::Verified => "verified",
            IntegrityStatus::Untrusted => "untrusted",
            IntegrityStatus::Unknown => "unknown",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "verified" => IntegrityStatus::Verified,
            "untrusted" => IntegrityStatus::Untrusted,
            _ => IntegrityStatus::Unknown,
        }
    }
}

/// Model row as stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRow {
    /// Cross-provider unique id. For catalog-seeded local models this
    /// matches the catalog entry id; for API providers it is
    /// `<provider_id>:<remote_model_id>` (plan §"Datenmodell").
    pub id: String,
    pub provider_id: Uuid,
    pub name: String,
    pub context_window: Option<i64>,
    /// Epoch milliseconds. `None` for entries seeded from the built-in
    /// catalog before any live refresh.
    pub fetched_at: Option<i64>,
    /// HuggingFace repo the tokenizer.json lives in. Required for
    /// local GGUFs at `load_local_model` time. `None` for api_key /
    /// cli_delegate rows — those do not tokenize on-device.
    pub tokenizer_repo: Option<String>,
    /// HuggingFace repository id (`owner/name`) this file was installed
    /// from. `None` for catalog/imported/provider rows.
    pub hf_repo: Option<String>,
    /// The exact GGUF filename selected inside `hf_repo`. `None` unless
    /// `hf_repo` is set.
    pub hf_filename: Option<String>,
    /// Commit SHA resolved and used for the install/update that produced
    /// the current local file. `None` unless `hf_repo` is set.
    pub hf_revision: Option<String>,
    /// Optional mutable branch/tag tracked for update checks. `None` for
    /// a direct SHA pin or a non-HF row.
    pub hf_revision_ref: Option<String>,
    /// Full SHA-256 hash of the local file's content, set only after a
    /// successful atomic download/update/import publish.
    pub file_sha256: Option<String>,
    pub integrity_status: IntegrityStatus,
    pub source_kind: SourceKind,
    /// What this model supports (spec 012). Stored as JSON in
    /// `capabilities_json`; `None` means not determined — never "unsupported".
    pub capabilities: Option<ModelCapabilities>,
}

const SELECT_COLUMNS: &str = "id, provider_id, name, context_window, fetched_at, tokenizer_repo, \
     hf_repo, hf_filename, hf_revision, hf_revision_ref, file_sha256, integrity_status, source_kind, \
     capabilities_json";

/// Inserts or updates a model row while preserving CRDT column metadata on
/// conflicts.
///
/// The HF source, hash and status columns are overwritten unconditionally
/// (unlike `tokenizer_repo`'s `COALESCE`) — every caller of this function
/// passes the authoritative value for that write (a provider refresh's
/// `None`s, or a download/update/import's freshly computed source +
/// `file_sha256`). A narrower in-place status change (e.g. the integrity
/// override) uses [`set_integrity_status`] instead of a full upsert.
///
/// `capabilities_json` is overwritten unconditionally too: a refresh
/// replaces what was known about a model entirely, so a stale answer is
/// never merged with a newer one. An undetermined record is stored as SQL
/// `NULL`, not as a JSON object of nulls.
pub fn upsert_model(tx: &mut CrdtTransaction<'_>, m: &ModelRow) -> haex_crdt::Result<usize> {
    let capabilities_json = capabilities_to_column(m.capabilities.as_ref())?;
    tx.execute(
        "INSERT INTO models \
           (id, provider_id, name, context_window, fetched_at, tokenizer_repo, \
            hf_repo, hf_filename, hf_revision, hf_revision_ref, file_sha256, \
            integrity_status, source_kind, capabilities_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) \
         ON CONFLICT(id) DO UPDATE SET \
           provider_id = excluded.provider_id, \
           name = excluded.name, \
           context_window = excluded.context_window, \
           fetched_at = excluded.fetched_at, \
           tokenizer_repo = COALESCE(excluded.tokenizer_repo, tokenizer_repo), \
           hf_repo = excluded.hf_repo, \
           hf_filename = excluded.hf_filename, \
           hf_revision = excluded.hf_revision, \
           hf_revision_ref = excluded.hf_revision_ref, \
           file_sha256 = excluded.file_sha256, \
           integrity_status = excluded.integrity_status, \
           source_kind = excluded.source_kind, \
           capabilities_json = excluded.capabilities_json",
        params![
            m.id,
            m.provider_id.to_string(),
            m.name,
            m.context_window,
            m.fetched_at,
            m.tokenizer_repo,
            m.hf_repo,
            m.hf_filename,
            m.hf_revision,
            m.hf_revision_ref,
            m.file_sha256,
            m.integrity_status.as_str(),
            m.source_kind.as_str(),
            capabilities_json,
        ],
    )
}

/// Serializes a record for `capabilities_json`; an undetermined (or absent)
/// record becomes SQL `NULL`.
fn capabilities_to_column(capabilities: Option<&ModelCapabilities>) -> Result<Option<String>> {
    capabilities
        .filter(|c| !c.is_undetermined())
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| haex_crdt::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

/// Writes what was found about a model's tool use into its record, keeping every other field.
/// A value of lower rank than the stored one is dropped (spec 032 FR-015: provider and catalog
/// beat the template finding, which beats a self-test). Returns whether the row changed.
///
/// A read-modify-write of `capabilities_json`, not an upsert: the rest of the record (reasoning,
/// attachments) came from another source and must survive.
pub fn set_tool_use(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    tool_use: ToolUse,
) -> haex_crdt::Result<bool> {
    let stored: Vec<Option<String>> = tx.query_map(
        "SELECT capabilities_json FROM models WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    let Some(raw) = stored.into_iter().next() else {
        return Ok(false);
    };
    let mut capabilities = capabilities_from_column(id, raw).unwrap_or_default();
    if capabilities
        .tool_use
        .is_some_and(|current| !tool_use.may_replace(&current))
    {
        return Ok(false);
    }
    capabilities.tool_use = Some(tool_use);
    let json = capabilities_to_column(Some(&capabilities))?;
    Ok(tx.execute(
        "UPDATE models SET capabilities_json = ?1 WHERE id = ?2",
        params![json, id],
    )? > 0)
}

/// What the user stated about a model, if anything.
fn user_stated(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
) -> haex_crdt::Result<Option<UserCapabilities>> {
    let stored: Vec<Option<String>> = tx.query_map(
        "SELECT capabilities_json FROM models WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    Ok(stored
        .into_iter()
        .next()
        .and_then(|raw| capabilities_from_column(id, raw))
        .and_then(|capabilities| capabilities.user)
        .filter(|user| !user.is_empty()))
}

/// Stores what the user states about a model and lays it over the record, keeping the rest of
/// it. An empty statement withdraws the earlier one together with the values it set. Returns the
/// new record, or `None` when the model does not exist.
pub fn set_user_capabilities(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    user: UserCapabilities,
) -> haex_crdt::Result<Option<ModelCapabilities>> {
    let stored: Vec<Option<String>> = tx.query_map(
        "SELECT capabilities_json FROM models WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    let Some(raw) = stored.into_iter().next() else {
        return Ok(None);
    };
    let capabilities = capabilities_from_column(id, raw).unwrap_or_default();
    let capabilities = if user.is_empty() {
        capabilities.without_user()
    } else {
        let mut capabilities = capabilities.without_user();
        capabilities.user = Some(user);
        capabilities.with_user_applied()
    };
    let json = capabilities_to_column(Some(&capabilities))?;
    tx.execute(
        "UPDATE models SET capabilities_json = ?1 WHERE id = ?2",
        params![json, id],
    )?;
    Ok(Some(capabilities))
}

/// Reads `capabilities_json` leniently: a value that cannot be parsed reads
/// as "not determined" and is logged with the model id and the parse error,
/// so one corrupt row can never make the model list fail to load (spec 012
/// FR-021). The parsed record is normalized, restoring the "presets are
/// never empty" invariant that serde bypasses.
pub(crate) fn capabilities_from_column(
    model_id: &str,
    raw: Option<String>,
) -> Option<ModelCapabilities> {
    let raw = raw?;
    match serde_json::from_str::<ModelCapabilities>(&raw) {
        Ok(capabilities) => Some(capabilities.normalized()),
        Err(error) => {
            log::warn!("ignoring unreadable capabilities_json for model {model_id}: {error}");
            None
        }
    }
}

/// Lists all models for a provider, ordered by name.
pub fn list_models_by_provider(
    q: &mut impl Query,
    provider_id: Uuid,
) -> haex_crdt::Result<Vec<ModelRow>> {
    q.query_map(
        &format!("SELECT {SELECT_COLUMNS} FROM models WHERE provider_id = ?1 ORDER BY name ASC"),
        params![provider_id.to_string()],
        row_to_model,
    )
}

/// Lists all models across every provider.
pub fn list_all_models(q: &mut impl Query) -> haex_crdt::Result<Vec<ModelRow>> {
    q.query_map(
        &format!("SELECT {SELECT_COLUMNS} FROM models ORDER BY name ASC"),
        &[],
        row_to_model,
    )
}

/// Reads a single model row by id, or `None` when missing.
pub fn get_model(q: &mut impl Query, id: &str) -> haex_crdt::Result<Option<ModelRow>> {
    q.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM models WHERE id = ?1"),
        params![id],
        row_to_model,
    )
}

/// Narrowly updates `integrity_status` without touching any other column —
/// in particular never `file_sha256` (research.md Entscheidung 6: "Das
/// Setzen einer neuen erwarteten SHA ist nur Bestandteil eines
/// erfolgreichen Downloads, Updates oder Imports; ein Load darf sie
/// niemals aktualisieren"). Used both by a successful pre-load hash match
/// (-> `Verified`) and by the explicit `load_model_with_integrity_override`
/// path (-> `Untrusted`).
pub fn set_integrity_status(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    status: IntegrityStatus,
) -> haex_crdt::Result<usize> {
    tx.execute(
        "UPDATE models SET integrity_status = ?1 WHERE id = ?2",
        params![status.as_str(), id],
    )
}

/// Records a successful upstream revision replacement for the same model
/// id: new commit SHA, new file hash, status reset to `verified`. Does not
/// touch `hf_revision_ref` — the tracked branch/tag is unchanged by an
/// update. Only called after the new file has been atomically published
/// (data-model.md "Ein Update ersetzt die Datei unter derselben Modell-ID
/// erst nach erfolgreicher atomarer Veröffentlichung").
pub fn update_hf_revision_and_hash(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    new_revision: &str,
    new_file_sha256: &str,
) -> haex_crdt::Result<usize> {
    tx.execute(
        "UPDATE models SET hf_revision = ?1, file_sha256 = ?2, integrity_status = ?3 WHERE id = ?4",
        params![
            new_revision,
            new_file_sha256,
            IntegrityStatus::Verified.as_str(),
            id
        ],
    )
}

/// Replaces the entire model cache for a provider: every entry in
/// `fresh` is upserted, then rows for `provider_id` that are absent
/// from `fresh` are removed. Run it in one `write` so a partial refresh
/// cannot leave the cache in an inconsistent state.
///
/// Order matters, and so does upserting rather than delete-then-insert.
/// `models` is CRDT-tracked: a DELETE fires the BEFORE-DELETE trigger,
/// which appends a tombstone to `haex_deleted_rows` carrying
/// the transaction HLC. That value is pinned per transaction, so a
/// re-INSERT of the same id in the same transaction lands on the *same*
/// HLC — and `delete_shadows_insert` resolves that tie in favour of the
/// delete, meaning a peer applying the payload would drop every
/// refreshed row. Touching only the rows that actually changed keeps
/// the delete-log free of tombstones for surviving models.
///
/// Upserting also tolerates a provider that reports the same model id
/// twice (cursor pagination can overlap when the remote catalog changes
/// mid-listing) — a plain INSERT would abort the whole refresh on the
/// primary-key conflict.
///
/// A failed remote fetch never reaches this function — callers only
/// invoke it on success, keeping the previous cache intact on error
/// per plan §"Anbietermodelle" ("Fehlgeschlagener Abruf erhält den
/// vorhandenen Cache").
pub fn replace_provider_models(
    tx: &mut CrdtTransaction<'_>,
    provider_id: Uuid,
    fresh: &[ModelRow],
) -> haex_crdt::Result<()> {
    for m in fresh {
        // One thing survives a refresh: what the user stated about the model (spec 055 FR-019).
        // The provider's answer is merged under it; nothing else of the old record is kept.
        match user_stated(tx, &m.id)? {
            Some(user) => {
                let mut row = m.clone();
                let mut capabilities = row.capabilities.take().unwrap_or_default();
                capabilities.user = Some(user);
                row.capabilities = Some(capabilities.with_user_applied());
                upsert_model(tx, &row)?;
            }
            None => {
                upsert_model(tx, m)?;
            }
        }
    }

    let keep: HashSet<&str> = fresh.iter().map(|m| m.id.as_str()).collect();
    let stale: Vec<String> = tx
        .query_map(
            "SELECT id FROM models WHERE provider_id = ?1",
            params![provider_id.to_string()],
            |r| r.get::<_, String>(0),
        )?
        .into_iter()
        .filter(|id| !keep.contains(id.as_str()))
        .collect();
    for id in stale {
        tx.execute("DELETE FROM models WHERE id = ?1", params![id])?;
    }
    Ok(())
}

/// Fills `tokenizer_repo` for the ids in `catalog` that still have the
/// column NULL. Idempotent — the `WHERE tokenizer_repo IS NULL` clause
/// filters out any row already populated, and a second pass updates
/// nothing.
///
/// Driven by the catalog rather than by a scan of the table: rows
/// written by `replace_provider_models` keep `tokenizer_repo` NULL
/// forever (API providers tokenize server-side), so scanning for NULLs
/// would re-read every remote model on every call. Bounded by the
/// catalog size, which is what makes this safe during open-time maintenance.
pub fn backfill_tokenizer_repo(
    tx: &mut CrdtTransaction<'_>,
    catalog: &[(&str, &str)],
) -> haex_crdt::Result<usize> {
    let mut updated = 0usize;
    for (id, repo) in catalog {
        updated += tx.execute(
            "UPDATE models SET tokenizer_repo = ?1 WHERE id = ?2 AND tokenizer_repo IS NULL",
            params![repo, id],
        )?;
    }
    Ok(updated)
}

/// Reclassifies pre-`0015` local-provider rows from the migration default
/// `source_kind = 'provider'` into `catalog` or `imported`. A SQL-only
/// migration cannot see the compiled catalog id list, so this mirrors the
/// existing [`backfill_tokenizer_repo`] idiom: driven by the (small,
/// compile-time) catalog rather than a full-table scan, called from
/// open-time maintenance. Idempotent — every
/// `WHERE` clause re-selects only rows still at the migration default.
///
/// Rows under an `api_key` provider are never touched: the `provider_id =
/// ?1` (local provider) predicate excludes them, so a true provider row
/// keeps `source_kind = 'provider'` regardless of its id.
pub fn backfill_source_kind(
    tx: &mut CrdtTransaction<'_>,
    local_provider_id: Uuid,
    catalog_ids: &[&str],
) -> haex_crdt::Result<usize> {
    let local_provider = local_provider_id.to_string();
    let mut updated = 0usize;
    for id in catalog_ids {
        updated += tx.execute(
            "UPDATE models SET source_kind = 'catalog' \
             WHERE id = ?1 AND provider_id = ?2 AND source_kind = 'provider'",
            params![id, local_provider],
        )?;
    }
    // `NOT IN ()` is invalid SQLite syntax for an empty list — fall back
    // to the unconditional form when there is nothing to exclude.
    let sql = if catalog_ids.is_empty() {
        "UPDATE models SET source_kind = 'imported' \
         WHERE provider_id = ?1 AND source_kind = 'provider'"
            .to_string()
    } else {
        let placeholders = std::iter::repeat_n("?", catalog_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "UPDATE models SET source_kind = 'imported' \
             WHERE provider_id = ?1 AND source_kind = 'provider' AND id NOT IN ({placeholders})"
        )
    };
    let mut params_vec: Vec<&dyn haex_crdt::rusqlite::ToSql> = vec![&local_provider];
    for id in catalog_ids {
        params_vec.push(id);
    }
    updated += tx.execute(&sql, params_vec.as_slice())?;
    Ok(updated)
}

/// Fills `capabilities_json` for local-provider rows that still have it
/// `NULL` — models registered before migration `0018`, which have no
/// provider refresh to fill them. A sibling of [`backfill_source_kind`]:
/// idempotent (only rows still at `NULL` are selected) and driven by the
/// local provider's own rows, so `api_key`/`cli_delegate` rows — whose
/// `NULL` honestly means "not refreshed yet" — are never touched.
///
/// Runs during open-time maintenance, so the installed-model listing remains
/// a read-only hot path.
pub fn backfill_local_capabilities(
    tx: &mut CrdtTransaction<'_>,
    local_provider_id: Uuid,
) -> haex_crdt::Result<usize> {
    let pending: Vec<String> = tx.query_map(
        "SELECT id FROM models WHERE provider_id = ?1 AND capabilities_json IS NULL",
        params![local_provider_id.to_string()],
        |r| r.get(0),
    )?;
    let mut updated = 0usize;
    for id in pending {
        let json = capabilities_to_column(Some(&ModelCapabilities::local(&id)))?;
        updated += tx.execute(
            "UPDATE models SET capabilities_json = ?1 WHERE id = ?2 AND capabilities_json IS NULL",
            params![json, id],
        )?;
    }
    Ok(updated)
}

fn row_to_model(row: &haex_crdt::rusqlite::Row<'_>) -> Result<ModelRow> {
    let integrity_status_str: String = row.get(11)?;
    let source_kind_str: String = row.get(12)?;
    let id: String = row.get(0)?;
    let capabilities = capabilities_from_column(&id, row.get(13)?);
    Ok(ModelRow {
        id,
        provider_id: uuid::from_row(row, 1)?,
        name: row.get(2)?,
        context_window: row.get(3)?,
        fetched_at: row.get(4)?,
        tokenizer_repo: row.get(5)?,
        hf_repo: row.get(6)?,
        hf_filename: row.get(7)?,
        hf_revision: row.get(8)?,
        hf_revision_ref: row.get(9)?,
        file_sha256: row.get(10)?,
        integrity_status: IntegrityStatus::from_str(&integrity_status_str),
        source_kind: SourceKind::from_str(&source_kind_str),
        capabilities,
    })
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod models_tests;

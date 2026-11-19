//! The run log of a rule (FR-043 to FR-046, data-model.md): each run with its cause, result and
//! counters, and every file action of it. It stays on this device, holds no file content, and keeps
//! the latest runs of each rule up to the log depth of this device (setting `syncRules.logDepth`).
//! Named `run_log` so it does not hide the `log` crate.

use haex_crdt::rusqlite::{params, Row};
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::{parse, stored, SyncRuleOutcome, SyncRuleSide};
use super::place::Trashed;
use super::run::{RunReport, RunTimings};
use crate::error::Result;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

/// The device setting with the number of runs kept per rule.
pub const LOG_DEPTH_KEY: &str = "syncRules.logDepth";
/// The log depth when the setting is missing.
pub const DEFAULT_LOG_DEPTH: u32 = 10;
/// The largest log depth.
pub const MAX_LOG_DEPTH: u32 = 100;

/// Why a run started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleRunCause {
    /// Its interval passed, or a place it waited for came back.
    Interval,
    /// A watched place changed.
    Watch,
    /// The user asked, or created or changed the rule.
    Manual,
    /// The vault opened.
    Startup,
}

/// What a run did with a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleRunAction {
    Copy,
    Move,
    Delete,
    ConflictCopy,
    Adopt,
    Skip,
    Error,
}

/// Where a mirrored deletion went (FR-046).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleTrashKind {
    /// The system's trash; holzi cannot open it.
    System,
    /// The rule trash of the place.
    Rule,
}

/// How another device was reached (a place on it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SyncRuleRoute {
    /// Straight to the device.
    Direct,
    /// By a relay.
    Relay,
}

/// One file action of a run, as the run notes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunItem {
    pub path: String,
    pub action: SyncRuleRunAction,
    /// Where it happened.
    pub side: SyncRuleSide,
    /// The reason of a skip or an error, the target of a move, the name of a conflict copy.
    pub detail: Option<String>,
    pub trash: Option<Trashed>,
}

impl RunItem {
    pub fn new(action: SyncRuleRunAction, side: SyncRuleSide, path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            action,
            side,
            detail: None,
            trash: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// A run in the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleRun {
    pub id: String,
    /// Milliseconds since 1970.
    #[ts(type = "number")]
    pub started_ms: i64,
    #[ts(type = "number")]
    pub duration_ms: i64,
    pub cause: SyncRuleRunCause,
    pub outcome: SyncRuleOutcome,
    pub error: Option<String>,
    pub copied: u32,
    pub moved: u32,
    pub deleted: u32,
    pub conflicts: u32,
    pub errors: u32,
    pub skipped: u32,
    /// Milliseconds spent listing the places.
    #[ts(type = "number")]
    pub scan_ms: i64,
    /// Milliseconds spent reading files for their hash.
    #[ts(type = "number")]
    pub hash_ms: i64,
    /// Milliseconds spent on the steps of the plan.
    #[ts(type = "number")]
    pub copy_ms: i64,
    /// The bytes of the files copied.
    #[ts(type = "number")]
    pub bytes: i64,
    /// How another device was reached; none for places on this device and for runs before this was
    /// noted.
    pub route: Option<SyncRuleRoute>,
    /// The round trip time to that device.
    pub rtt_ms: Option<u32>,
}

/// A file action of a run in the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SyncRuleRunItem {
    pub seq: u32,
    pub path: String,
    pub action: SyncRuleRunAction,
    pub side: SyncRuleSide,
    pub detail: Option<String>,
    pub trash_kind: Option<SyncRuleTrashKind>,
    /// The path in the rule trash of the place.
    pub trash_path: Option<String>,
}

/// A finished run, ready to be written. Its log entry has an id of its own: the run id names the
/// rule trash folder and may come again (tests).
#[derive(Debug, Clone)]
pub struct Finished {
    pub id: String,
    pub cause: SyncRuleRunCause,
    pub started_ms: i64,
    pub duration_ms: i64,
    pub outcome: SyncRuleOutcome,
    pub error: Option<String>,
    pub copied: u64,
    pub moved: u64,
    pub deleted: u64,
    pub conflicts: u64,
    pub errors: u64,
    pub skipped: u64,
    pub timings: RunTimings,
    pub items: Vec<RunItem>,
}

impl Finished {
    pub fn of(
        cause: SyncRuleRunCause,
        started_ms: i64,
        duration_ms: i64,
        report: &RunReport,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            cause,
            started_ms,
            duration_ms,
            outcome: report.outcome,
            error: report.error.clone(),
            copied: report.counts.copied,
            moved: report.counts.moved,
            deleted: report.counts.deleted,
            conflicts: report.counts.conflicts,
            errors: report.counts.errors,
            skipped: report.skipped.len() as u64,
            timings: report.timings,
            items: report.items.clone(),
        }
    }
}

/// Whether `value` is a log depth the setting takes: a whole number from 0 to 100.
pub fn is_valid_depth(value: &str) -> bool {
    value
        .parse::<u32>()
        .is_ok_and(|depth| depth <= MAX_LOG_DEPTH && value == depth.to_string())
}

fn scope(device: &str) -> Result<PrefScope> {
    let uuid =
        uuid::Uuid::parse_str(device).map_err(|_| crate::error::HaexError::InvalidInput {
            reason: "the device is no UUID".to_owned(),
        })?;
    Ok(PrefScope::Device(uuid))
}

/// The log depth of `device`.
pub fn depth(q: &mut impl Query, device: &str) -> Result<u32> {
    let value = preferences::get(q, scope(device)?, LOG_DEPTH_KEY)?;
    Ok(value
        .filter(|value| is_valid_depth(value))
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_LOG_DEPTH))
}

/// Sets the log depth of `device` and trims the log of each of its rules at once (FR-044).
pub fn set_depth(tx: &mut CrdtTransaction<'_>, device: &str, depth: u32) -> Result<()> {
    preferences::insert_or_update(tx, scope(device)?, LOG_DEPTH_KEY, &depth.to_string())?;
    let rules: Vec<String> = tx.query_map(
        "SELECT id FROM haex_sync_rules_no_sync WHERE vault_device_uuid = ?1",
        &[&device],
        |r| r.get(0),
    )?;
    for rule in rules {
        trim(tx, device, &rule, depth)?;
    }
    Ok(())
}

/// Writes a finished run of `rule_id` and keeps the latest runs up to the log depth; with depth 0
/// nothing is written.
pub fn record(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    rule_id: &str,
    run: &Finished,
) -> Result<()> {
    let depth = depth(tx, device)?;
    if depth == 0 {
        return trim(tx, device, rule_id, 0);
    }
    tx.execute(
        "INSERT INTO haex_sync_runs_no_sync (id, rule_id, vault_device_uuid, started_ms, \
         duration_ms, cause, outcome, error, copied, moved, deleted, conflicts, errors, skipped, \
         scan_ms, hash_ms, copy_ms, bytes, route, rtt_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, \
         ?19, ?20)",
        params![
            run.id,
            rule_id,
            device,
            run.started_ms,
            run.duration_ms,
            stored(&run.cause),
            stored(&run.outcome),
            run.error,
            run.copied as i64,
            run.moved as i64,
            run.deleted as i64,
            run.conflicts as i64,
            run.errors as i64,
            run.skipped as i64,
            run.timings.scan_ms as i64,
            run.timings.hash_ms as i64,
            run.timings.copy_ms as i64,
            run.timings.bytes as i64,
            run.timings.route.map(|route| stored(&if route.relay {
                SyncRuleRoute::Relay
            } else {
                SyncRuleRoute::Direct
            })),
            run.timings.route.map(|route| i64::from(route.rtt_ms)),
        ],
    )?;
    for (seq, item) in run.items.iter().enumerate() {
        let (kind, trash_path) = match &item.trash {
            Some(Trashed::System) => (Some(stored(&SyncRuleTrashKind::System)), None),
            Some(Trashed::Rule(path)) => (Some(stored(&SyncRuleTrashKind::Rule)), Some(path)),
            None => (None, None),
        };
        tx.execute(
            "INSERT INTO haex_sync_run_items_no_sync (run_id, seq, path, action, side, detail, \
             trash_kind, trash_path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run.id,
                seq as i64,
                item.path,
                stored(&item.action),
                stored(&item.side),
                item.detail,
                kind,
                trash_path,
            ],
        )?;
    }
    trim(tx, device, rule_id, depth)
}

/// Keeps the latest `depth` runs of `rule_id`; the files of the others go with them.
pub fn trim(tx: &mut CrdtTransaction<'_>, device: &str, rule_id: &str, depth: u32) -> Result<()> {
    let old =
        "SELECT id FROM haex_sync_runs_no_sync WHERE rule_id = ?1 AND vault_device_uuid = ?2 \
               AND id NOT IN (SELECT id FROM haex_sync_runs_no_sync WHERE rule_id = ?1 AND \
               vault_device_uuid = ?2 ORDER BY started_ms DESC, rowid DESC LIMIT ?3)";
    tx.execute(
        &format!("DELETE FROM haex_sync_run_items_no_sync WHERE run_id IN ({old})"),
        params![rule_id, device, depth],
    )?;
    tx.execute(
        &format!("DELETE FROM haex_sync_runs_no_sync WHERE id IN ({old})"),
        params![rule_id, device, depth],
    )?;
    Ok(())
}

fn count(r: &Row<'_>, index: usize) -> haex_crdt::rusqlite::Result<u32> {
    Ok(u32::try_from(r.get::<_, i64>(index)?).unwrap_or(u32::MAX))
}

/// The logged runs of `rule_id`, the latest first.
pub fn runs(q: &mut impl Query, device: &str, rule_id: &str) -> Result<Vec<SyncRuleRun>> {
    Ok(q.query_map(
        "SELECT id, started_ms, duration_ms, cause, outcome, error, copied, moved, deleted, \
         conflicts, errors, skipped, scan_ms, hash_ms, copy_ms, bytes, route, rtt_ms \
         FROM haex_sync_runs_no_sync \
         WHERE rule_id = ?1 AND vault_device_uuid = ?2 ORDER BY started_ms DESC, rowid DESC",
        &[&rule_id, &device],
        |r| {
            let (cause, outcome) = (
                parse::<SyncRuleRunCause>(&r.get::<_, String>(3)?),
                parse::<SyncRuleOutcome>(&r.get::<_, String>(4)?),
            );
            let (Some(cause), Some(outcome)) = (cause, outcome) else {
                return Ok(None);
            };
            Ok(Some(SyncRuleRun {
                id: r.get(0)?,
                started_ms: r.get(1)?,
                duration_ms: r.get(2)?,
                cause,
                outcome,
                error: r.get(5)?,
                copied: count(r, 6)?,
                moved: count(r, 7)?,
                deleted: count(r, 8)?,
                conflicts: count(r, 9)?,
                errors: count(r, 10)?,
                skipped: count(r, 11)?,
                scan_ms: r.get(12)?,
                hash_ms: r.get(13)?,
                copy_ms: r.get(14)?,
                bytes: r.get(15)?,
                route: r
                    .get::<_, Option<String>>(16)?
                    .and_then(|route| parse::<SyncRuleRoute>(&route)),
                rtt_ms: r
                    .get::<_, Option<i64>>(17)?
                    .map(|ms| u32::try_from(ms).unwrap_or(u32::MAX)),
            }))
        },
    )?
    .into_iter()
    .flatten()
    .collect())
}

/// Up to `limit` file actions of the run `run_id` of `device`, from `offset` on.
pub fn items(
    q: &mut impl Query,
    device: &str,
    run_id: &str,
    offset: u32,
    limit: u32,
) -> Result<Vec<SyncRuleRunItem>> {
    Ok(q.query_map(
        "SELECT i.seq, i.path, i.action, i.side, i.detail, i.trash_kind, i.trash_path \
         FROM haex_sync_run_items_no_sync i JOIN haex_sync_runs_no_sync r ON r.id = i.run_id \
         WHERE i.run_id = ?1 AND r.vault_device_uuid = ?2 ORDER BY i.seq LIMIT ?3 OFFSET ?4",
        &[&run_id, &device, &limit, &offset],
        |r| {
            let action = parse::<SyncRuleRunAction>(&r.get::<_, String>(2)?);
            let side = parse::<SyncRuleSide>(&r.get::<_, String>(3)?);
            let (Some(action), Some(side)) = (action, side) else {
                return Ok(None);
            };
            Ok(Some(SyncRuleRunItem {
                seq: count(r, 0)?,
                path: r.get(1)?,
                action,
                side,
                detail: r.get(4)?,
                trash_kind: r
                    .get::<_, Option<String>>(5)?
                    .and_then(|kind| parse::<SyncRuleTrashKind>(&kind)),
                trash_path: r.get(6)?,
            }))
        },
    )?
    .into_iter()
    .flatten()
    .collect())
}

#[cfg(test)]
#[path = "run_log_tests.rs"]
mod tests;

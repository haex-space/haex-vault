//! One run of a rule (research R3): scan both places, guard against a known place that lists empty
//! (FR-022), plan, read the hashes the plan asks for and plan again, execute, and note the result
//! on the rule and in the run log (FR-043).

use std::collections::{BTreeMap, BTreeSet};

use tokio_util::sync::CancellationToken;

use super::conflict::conflict_path;
use super::execute::{execute, stops_the_run, Counts, Stopped, Target, TrashedFile, MAX_FAILURES};
use super::ignore::LineError;
use super::model::{SyncRule, SyncRuleOutcome, SyncRuleSide};
use super::place::{Place, SkipReason};
use super::place_file::check_file_source;
use super::plan::{build, Hashes, Inputs, Plan, Step};
use super::progress::Reporter;
use super::run_log::{self, Finished, RunItem, SyncRuleRunAction, SyncRuleRunCause};
use super::scan::{scan, ScanError, Scanned};
use super::store::{self, PendingRow, StateRow};
use crate::files::FilesErrorCode;
use crate::vault_gate::VaultDb;

/// Who runs a rule, and when.
pub struct RunContext<'a> {
    pub db: &'a VaultDb,
    /// The `vault_device_uuid` of this device.
    pub device: &'a str,
    /// The name of this device in conflict copies.
    pub device_name: &'a str,
    /// Today as `2026-10-27`, for conflict copies.
    pub today: &'a str,
    /// The time the run ends with, ISO 8601.
    pub now: &'a (dyn Fn() -> String + Sync),
    pub run_id: &'a str,
    pub cause: SyncRuleRunCause,
    pub cancel: &'a CancellationToken,
    pub progress: &'a Reporter,
}

/// What one run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub outcome: SyncRuleOutcome,
    /// Why the run waits or failed.
    pub error: Option<String>,
    pub counts: Counts,
    pub failures: Vec<(String, String)>,
    pub skipped: Vec<(String, SkipReason)>,
    pub ignore_errors: Vec<LineError>,
    pub ignored: usize,
    pub trashed: Vec<TrashedFile>,
    pub conflict_copies: Vec<String>,
    /// Every file action, for the log.
    pub items: Vec<RunItem>,
}

impl RunReport {
    pub(super) fn stopped(outcome: SyncRuleOutcome, error: impl Into<String>) -> Self {
        Self {
            outcome,
            error: Some(error.into()),
            counts: Counts::default(),
            failures: Vec::new(),
            skipped: Vec::new(),
            ignore_errors: Vec::new(),
            ignored: 0,
            trashed: Vec::new(),
            conflict_copies: Vec::new(),
            items: Vec::new(),
        }
    }
}

/// When a run started.
#[derive(Debug, Clone, Copy)]
pub struct Started {
    ms: i64,
    at: std::time::Instant,
}

impl Started {
    pub fn now() -> Self {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            });
        Self {
            ms,
            at: std::time::Instant::now(),
        }
    }
}

/// Runs `rule` between `source` and `target` and notes the result on the rule and in the log.
pub async fn run(
    ctx: &RunContext<'_>,
    rule: &SyncRule,
    source: &dyn Place,
    target: &dyn Place,
    started: Started,
) -> RunReport {
    let report = run_inner(ctx, rule, source, target).await;
    note(ctx, rule, &report, started).await;
    report
}

/// Notes how a run that started at `started` ended: on the rule, and in the log.
pub async fn note(ctx: &RunContext<'_>, rule: &SyncRule, report: &RunReport, started: Started) {
    let duration = i64::try_from(started.at.elapsed().as_millis()).unwrap_or(i64::MAX);
    let finished = Finished::of(ctx.cause, started.ms, duration, report);
    let (device, id, at, outcome, error) = (
        ctx.device.to_owned(),
        rule.id.clone(),
        (ctx.now)(),
        report.outcome,
        report.error.clone(),
    );
    let written = ctx
        .db
        .write(move |tx| {
            store::set_result(tx, &device, &id, &at, outcome, error.as_deref())?;
            run_log::record(tx, &device, &id, &finished)?;
            Ok(())
        })
        .await;
    if let Err(error) = written {
        log::warn!("file_sync: the result of a run could not be written: {error}");
    }
}

async fn run_inner(
    ctx: &RunContext<'_>,
    rule: &SyncRule,
    source: &dyn Place,
    target: &dyn Place,
) -> RunReport {
    // A file rule checks its source file first: with none at the path, nothing is scanned, copied or
    // deleted (spec 074 FR-006).
    if rule.is_file_rule() {
        if let Err(error) = check_file_source(rule, source).await {
            return stopped_by(error);
        }
    }
    ctx.progress.scanning();
    let scanned = match scan(source, target, ctx.cancel).await {
        Ok(scanned) => scanned,
        Err(error) => {
            if ctx.cancel.is_cancelled() {
                return RunReport::stopped(SyncRuleOutcome::Cancelled, "cancelled");
            }
            return stopped_by(error);
        }
    };
    let (device, id) = (ctx.device.to_owned(), rule.id.clone());
    let (state, pending) = match ctx
        .db
        .read(move |r| {
            Ok((
                store::state(r, &device, &id)?,
                store::pending(r, &device, &id)?,
            ))
        })
        .await
    {
        Ok(read) => read,
        Err(error) => {
            return RunReport::stopped(SyncRuleOutcome::Error, format!("state: {error}"));
        }
    };
    if let Some(side) = emptied(rule, &scanned, &state) {
        return RunReport::stopped(
            SyncRuleOutcome::Waiting,
            format!(
                "{}: the place lists no files although it held some",
                name(side)
            ),
        );
    }
    if ctx.cancel.is_cancelled() {
        return RunReport::stopped(SyncRuleOutcome::Cancelled, "cancelled");
    }
    let mut hashes = Hashes::new();
    let mut plan = plan_for(ctx.device_name, ctx.today, rule, &scanned, &state, &hashes);
    let mut failures = Vec::new();
    let mut items = Vec::new();
    let mut hash_errors = 0u64;
    if !plan.hash_reads.is_empty() {
        let mut stopped = None;
        let mut failures_in_a_row = 0u32;
        for (side, path) in &plan.hash_reads {
            let place = match side {
                SyncRuleSide::Source => source,
                SyncRuleSide::Target => target,
            };
            match place.hash(path, ctx.cancel).await {
                Ok(hash) => {
                    hashes.insert((*side, path.clone()), hash);
                    failures_in_a_row = 0;
                }
                Err(error) => {
                    hash_errors += 1;
                    failures_in_a_row += 1;
                    if stops_the_run(error.code)
                        || failures_in_a_row >= super::execute::FAILURES_IN_A_ROW
                    {
                        stopped = Some(Stopped {
                            code: error.code,
                            reason: error.message.clone(),
                        });
                    }
                    items.push(
                        RunItem::new(SyncRuleRunAction::Error, *side, path.clone())
                            .detail(error.message.clone()),
                    );
                    if failures.len() < MAX_FAILURES {
                        failures.push((path.clone(), error.message));
                    }
                    if stopped.is_some() {
                        break;
                    }
                }
            }
        }
        if ctx.cancel.is_cancelled() {
            return RunReport::stopped(SyncRuleOutcome::Cancelled, "cancelled");
        }
        // Reading the hashes of a place that refuses is not asked again for every file.
        if let Some(stopped) = stopped {
            let mut report = RunReport::stopped(
                stopped_outcome(&stopped),
                format!("stopped while reading files: {}", stopped.reason),
            );
            report.counts.errors = hash_errors;
            report.failures = failures;
            report.items = items;
            return report;
        }
        plan = plan_for(ctx.device_name, ctx.today, rule, &scanned, &state, &hashes);
    }
    let executed = execute(
        &Target {
            db: ctx.db,
            device: ctx.device,
            rule,
            run_id: ctx.run_id,
            source,
            target,
            source_listing: &scanned.source,
            target_listing: &scanned.target,
            state: &state,
            cancel: ctx.cancel,
            progress: ctx.progress,
        },
        &plan.steps,
    )
    .await;
    let unhashed = hash_errors;
    failures.extend(executed.failures);
    failures.truncate(MAX_FAILURES);
    let mut counts = executed.counts;
    counts.errors += unhashed;
    if !executed.cancelled {
        drop_stale_pending(ctx, rule, &plan, &pending).await;
    }
    let outcome = if executed.cancelled {
        SyncRuleOutcome::Cancelled
    } else if let Some(stopped) = &executed.stopped {
        stopped_outcome(stopped)
    } else if counts.errors == 0 {
        SyncRuleOutcome::Done
    } else {
        SyncRuleOutcome::Partial
    };
    let mut skipped = scanned.source.skipped.clone();
    skipped.extend(scanned.target.skipped.iter().cloned());
    items.extend(executed.items);
    for (side, listing) in [
        (SyncRuleSide::Source, &scanned.source),
        (SyncRuleSide::Target, &scanned.target),
    ] {
        for (path, reason) in &listing.skipped {
            items.push(
                RunItem::new(SyncRuleRunAction::Skip, side, path.clone())
                    .detail(skip_reason(*reason)),
            );
        }
    }
    RunReport {
        outcome,
        error: match &executed.stopped {
            Some(stopped) => Some(format!(
                "stopped after {} failed files: {}",
                counts.errors, stopped.reason
            )),
            None => (outcome == SyncRuleOutcome::Partial)
                .then(|| format!("{} files failed", counts.errors)),
        },
        counts,
        failures,
        skipped,
        ignore_errors: scanned.exclusions.errors.clone(),
        ignored: plan.ignored,
        trashed: executed.trashed,
        conflict_copies: executed.conflict_copies,
        items,
    }
}

/// How a run that stops before it plans ends: a place that does not answer waits, one that refuses is an
/// error, a file rule without its file is an invalid source.
fn stopped_by(error: ScanError) -> RunReport {
    match error {
        ScanError::Unreachable { side, reason } => RunReport::stopped(
            SyncRuleOutcome::Waiting,
            format!("{}: {reason}", name(side)),
        ),
        ScanError::Failed { side, reason } => {
            RunReport::stopped(SyncRuleOutcome::Error, format!("{}: {reason}", name(side)))
        }
        ScanError::InvalidSource { reason } => {
            RunReport::stopped(SyncRuleOutcome::InvalidSource, reason)
        }
    }
}

/// A place that does not answer waits and is tried again later; one that refuses is an error the
/// user has to look at.
fn stopped_outcome(stopped: &Stopped) -> SyncRuleOutcome {
    match stopped.code {
        FilesErrorCode::Unreachable => SyncRuleOutcome::Waiting,
        _ => SyncRuleOutcome::Error,
    }
}

/// The plan of `rule` for what `scanned` shows; conflict copies are named after `device_name` and
/// `today`.
pub(super) fn plan_for(
    device_name: &str,
    today: &str,
    rule: &SyncRule,
    scanned: &Scanned,
    state: &[StateRow],
    hashes: &Hashes,
) -> Plan {
    // A conflict copy may land on either side, so its name is free on both.
    let conflict_name = |path: &str, taken: &[String]| {
        conflict_path(path, device_name, today, &|candidate| {
            scanned.source.files.contains_key(candidate)
                || scanned.target.files.contains_key(candidate)
                || taken.iter().any(|name| name == candidate)
        })
    };
    build(&Inputs {
        rule,
        source: &scanned.source,
        target: &scanned.target,
        state,
        exclusions: &scanned.exclusions,
        hashes,
        conflict_name: &conflict_name,
    })
}

/// Drops the waiting deletions this run no longer plans: the file came back, changed, or the delete
/// mode is another now (data-model.md).
async fn drop_stale_pending(
    ctx: &RunContext<'_>,
    rule: &SyncRule,
    plan: &Plan,
    pending: &[PendingRow],
) {
    let planned: BTreeSet<&str> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::Pending { path, .. } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    let stale: Vec<String> = pending
        .iter()
        .filter(|row| !planned.contains(row.path.as_str()))
        .map(|row| row.path.clone())
        .collect();
    if stale.is_empty() {
        return;
    }
    let (device, id) = (ctx.device.to_owned(), rule.id.clone());
    let written = ctx
        .db
        .write(move |tx| {
            for path in &stale {
                store::remove_pending(tx, &device, &id, path)?;
            }
            Ok(())
        })
        .await;
    if let Err(error) = written {
        log::warn!("file_sync: stale waiting deletions could not be dropped: {error}");
    }
}

/// The side that lists no files although the state knows files there (FR-022). An empty target
/// on a first run is fine.
fn emptied(rule: &SyncRule, scanned: &Scanned, state: &[StateRow]) -> Option<SyncRuleSide> {
    let mut known: BTreeMap<SyncRuleSide, bool> = BTreeMap::new();
    for row in state {
        if row.source.is_some() {
            known.insert(SyncRuleSide::Source, true);
        }
        if row.target.is_some() {
            known.insert(SyncRuleSide::Target, true);
        }
    }
    [
        (SyncRuleSide::Source, &scanned.source),
        (SyncRuleSide::Target, &scanned.target),
    ]
    .into_iter()
    .find(|(side, listing)| {
        // The copy of a file rule is copied again when it is gone: it is no emptied place.
        let copy_of_a_file_rule = rule.is_file_rule() && *side == SyncRuleSide::Target;
        !copy_of_a_file_rule && known.contains_key(side) && listing.files.is_empty()
    })
    .map(|(side, _)| side)
}

/// The detail of a skipped file in the log; the window translates it.
fn skip_reason(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Link => "link",
        SkipReason::NoAccess => "noAccess",
        SkipReason::BadName => "badName",
    }
}

fn name(side: SyncRuleSide) -> &'static str {
    match side {
        SyncRuleSide::Source => "source",
        SyncRuleSide::Target => "target",
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;

//! One run of a rule (research R3): scan both places, guard against a known place that lists empty
//! (FR-022), plan, read the hashes the plan asks for and plan again, execute, and note the result
//! on the rule.

use std::collections::BTreeMap;

use tokio_util::sync::CancellationToken;

use super::conflict::conflict_path;
use super::execute::{execute, stops_the_run, Counts, Stopped, Target, TrashedFile, MAX_FAILURES};
use super::ignore::LineError;
use super::model::{SyncRule, SyncRuleOutcome, SyncRuleSide};
use super::place::{Place, SkipReason};
use super::plan::{build, Hashes, Inputs, Plan};
use super::progress::Reporter;
use super::scan::{scan, ScanError, Scanned};
use super::store::{self, StateRow};
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
        }
    }
}

/// Runs `rule` between `a` and `b` and notes the result on the rule.
pub async fn run(ctx: &RunContext<'_>, rule: &SyncRule, a: &dyn Place, b: &dyn Place) -> RunReport {
    let report = run_inner(ctx, rule, a, b).await;
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
            Ok(())
        })
        .await;
    if let Err(error) = written {
        log::warn!("file_sync: the result of a run could not be written: {error}");
    }
    report
}

async fn run_inner(
    ctx: &RunContext<'_>,
    rule: &SyncRule,
    a: &dyn Place,
    b: &dyn Place,
) -> RunReport {
    ctx.progress.scanning();
    let scanned = match scan(a, b, ctx.cancel).await {
        Ok(scanned) => scanned,
        Err(error) => {
            if ctx.cancel.is_cancelled() {
                return RunReport::stopped(SyncRuleOutcome::Cancelled, "cancelled");
            }
            match error {
                ScanError::Unreachable { side, reason } => {
                    return RunReport::stopped(
                        SyncRuleOutcome::Waiting,
                        format!("{}: {reason}", name(side)),
                    );
                }
                ScanError::Failed { side, reason } => {
                    return RunReport::stopped(
                        SyncRuleOutcome::Error,
                        format!("{}: {reason}", name(side)),
                    );
                }
            }
        }
    };
    let (device, id) = (ctx.device.to_owned(), rule.id.clone());
    let state = match ctx
        .db
        .read(move |r| Ok(store::state(r, &device, &id)))
        .await
    {
        Ok(Ok(state)) => state,
        Ok(Err(error)) | Err(error) => {
            return RunReport::stopped(SyncRuleOutcome::Error, format!("state: {error}"));
        }
    };
    if let Some(side) = emptied(&scanned, &state) {
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
    let mut plan = plan_for(ctx, rule, &scanned, &state, &hashes);
    let mut failures = Vec::new();
    if !plan.hash_reads.is_empty() {
        let mut stopped = None;
        let mut failures_in_a_row = 0u32;
        for (side, path) in &plan.hash_reads {
            let place = match side {
                SyncRuleSide::A => a,
                SyncRuleSide::B => b,
            };
            match place.hash(path, ctx.cancel).await {
                Ok(hash) => {
                    hashes.insert((*side, path.clone()), hash);
                    failures_in_a_row = 0;
                }
                Err(error) => {
                    failures_in_a_row += 1;
                    if stops_the_run(error.code)
                        || failures_in_a_row >= super::execute::FAILURES_IN_A_ROW
                    {
                        stopped = Some(Stopped {
                            code: error.code,
                            reason: error.message.clone(),
                        });
                    }
                    failures.push((path.clone(), error.message));
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
            report.counts.errors = failures.len() as u64;
            failures.truncate(MAX_FAILURES);
            report.failures = failures;
            return report;
        }
        plan = plan_for(ctx, rule, &scanned, &state, &hashes);
    }
    let executed = execute(
        &Target {
            db: ctx.db,
            device: ctx.device,
            rule,
            run_id: ctx.run_id,
            a,
            b,
            listing_a: &scanned.a,
            listing_b: &scanned.b,
            state: &state,
            cancel: ctx.cancel,
            progress: ctx.progress,
        },
        &plan.steps,
    )
    .await;
    let unhashed = failures.len() as u64;
    failures.extend(executed.failures);
    failures.truncate(MAX_FAILURES);
    let mut counts = executed.counts;
    counts.errors += unhashed;
    let outcome = if executed.cancelled {
        SyncRuleOutcome::Cancelled
    } else if let Some(stopped) = &executed.stopped {
        stopped_outcome(stopped)
    } else if counts.errors == 0 {
        SyncRuleOutcome::Done
    } else {
        SyncRuleOutcome::Partial
    };
    let mut skipped = scanned.a.skipped.clone();
    skipped.extend(scanned.b.skipped.iter().cloned());
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

fn plan_for(
    ctx: &RunContext<'_>,
    rule: &SyncRule,
    scanned: &Scanned,
    state: &[StateRow],
    hashes: &Hashes,
) -> Plan {
    // A conflict copy may land on either side, so its name is free on both.
    let conflict_name = |path: &str, taken: &[String]| {
        conflict_path(path, ctx.device_name, ctx.today, &|candidate| {
            scanned.a.files.contains_key(candidate)
                || scanned.b.files.contains_key(candidate)
                || taken.iter().any(|name| name == candidate)
        })
    };
    build(&Inputs {
        rule,
        a: &scanned.a,
        b: &scanned.b,
        state,
        exclusions: &scanned.exclusions,
        hashes,
        conflict_name: &conflict_name,
    })
}

/// The side that lists no files although the state knows files there (FR-022). An empty target
/// on a first run is fine.
fn emptied(scanned: &Scanned, state: &[StateRow]) -> Option<SyncRuleSide> {
    let mut known: BTreeMap<SyncRuleSide, bool> = BTreeMap::new();
    for row in state {
        if row.a.is_some() {
            known.insert(SyncRuleSide::A, true);
        }
        if row.b.is_some() {
            known.insert(SyncRuleSide::B, true);
        }
    }
    [(SyncRuleSide::A, &scanned.a), (SyncRuleSide::B, &scanned.b)]
        .into_iter()
        .find(|(side, listing)| known.contains_key(side) && listing.files.is_empty())
        .map(|(side, _)| side)
}

fn name(side: SyncRuleSide) -> &'static str {
    match side {
        SyncRuleSide::A => "place A",
        SyncRuleSide::B => "place B",
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;

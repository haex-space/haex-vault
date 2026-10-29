//! The control loop of a vault session (research R8): which rule runs when. Every active rule runs
//! once after the vault opened (catch up), then after its interval, after a change in a watched
//! local place has been quiet for a moment, when the user asks, and, for a rule that waits for a
//! place, when the reachable devices change (FR-024). At most one run per rule and
//! [`MAX_RUNS`] runs at once; the loop and its runs end with the session.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::model::{SyncRule, SyncRuleOutcome};
use super::run::RunReport;
use super::run_log::SyncRuleRunCause;
use crate::files::SourceRef;

/// Runs of different rules at once.
pub const MAX_RUNS: usize = 2;
/// How long a watched place stays quiet before its rule runs.
pub const QUIET: Duration = Duration::from_secs(2);
/// The interval of a rule whose places are all on this device; the watcher does the rest.
pub const LOCAL_SAFETY: Duration = Duration::from_secs(60 * 60);
/// How often the loop looks at the clock.
const TICK: Duration = Duration::from_secs(1);

/// What the commands ask of the loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Runs the rule now ("Jetzt abgleichen").
    Run(String),
    /// Rules were created, changed, paused or deleted.
    Reload,
    /// Stops the running run of the rule.
    Cancel(String),
    /// Releases the guard installed while a cancellation is being coordinated.
    ReleaseCancel(String),
    /// The reachable devices changed; rules that wait for a place try again.
    Wake,
}

/// What the loop needs from the session.
#[async_trait]
pub trait Runner: Send + Sync + 'static {
    /// The rules of this device.
    async fn rules(&self) -> Vec<SyncRule>;
    /// One run of `rule`, started for `cause`.
    async fn run(
        &self,
        rule: SyncRule,
        cause: SyncRuleRunCause,
        cancel: CancellationToken,
    ) -> RunReport;
    /// Tells the window that a rule (or, with `None`, the list) changed.
    fn changed(&self, rule_id: Option<String>);
    /// Watches the local places of `rule` that its runs copy from; `notify` hears each change.
    /// `None` when there is nothing local to watch. Dropping the handle stops it.
    fn watch(&self, rule: &SyncRule, notify: Arc<dyn Fn() + Send + Sync>) -> Option<Box<dyn Send>>;
}

/// What the loop shares with the commands.
#[derive(Default)]
pub struct Shared {
    pub running: HashSet<String>,
    /// The report of each rule's last run in this session.
    pub last: HashMap<String, RunReport>,
    /// Rules whose cancellation has been requested before the command reaches the loop.
    pub cancel_requested: HashSet<String>,
    /// Cancellations the loop has received, for callers waiting before a destructive action.
    pub cancel_processed: HashSet<String>,
}

/// The interval of `rule` (research R8).
pub fn interval_of(rule: &SyncRule) -> Duration {
    let local = |source: &SourceRef| matches!(source, SourceRef::Device);
    if local(&rule.place_a.source) && local(&rule.place_b.source) {
        LOCAL_SAFETY
    } else {
        Duration::from_secs(u64::from(rule.interval_secs))
    }
}

/// Runs until `token` is cancelled.
pub async fn control<R: Runner>(
    runner: Arc<R>,
    mut commands: UnboundedReceiver<Command>,
    shared: Arc<Mutex<Shared>>,
    token: CancellationToken,
) {
    let (changes_tx, mut changes) = unbounded_channel::<String>();
    let mut rules = runner.rules().await;
    let mut watches = watch_all(&*runner, &rules, &changes_tx);
    let mut due: Vec<String> = Vec::new();
    // Why each due rule runs; the first reason wins.
    let mut causes: HashMap<String, SyncRuleRunCause> = HashMap::new();
    for rule in rules.iter().filter(|rule| rule.enabled) {
        queue(&mut due, &mut causes, &rule.id, SyncRuleRunCause::Startup);
    }
    let mut last_start: HashMap<String, Instant> = HashMap::new();
    let mut changed_at: BTreeMap<String, Instant> = BTreeMap::new();
    let mut cancels: HashMap<String, CancellationToken> = HashMap::new();
    let mut runs: JoinSet<(String, RunReport)> = JoinSet::new();
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            () = token.cancelled() => break,
            Some(command) = commands.recv() => match command {
                Command::Run(id) => queue(&mut due, &mut causes, &id, SyncRuleRunCause::Manual),
                Command::Reload => {
                    let previous = rules;
                    rules = runner.rules().await;
                    watches = watch_all(&*runner, &rules, &changes_tx);
                    // A rule that is new, active again, or reset by a place/direction change runs
                    // once now. The store clears its state for the latter, so waiting for the old
                    // interval would leave the new rule without its first meeting.
                    for rule in rules.iter().filter(|rule| rule.enabled) {
                        let before = previous.iter().find(|old| old.id == rule.id);
                        if starts_after_reload(before, rule) || !last_start.contains_key(&rule.id) {
                            queue(&mut due, &mut causes, &rule.id, SyncRuleRunCause::Manual);
                        }
                    }
                    runner.changed(None);
                }
                Command::Cancel(id) => {
                    if let Some(cancel) = cancels.get(&id) {
                        cancel.cancel();
                    }
                    due.retain(|other| other != &id);
                    causes.remove(&id);
                    shared
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .cancel_processed
                        .insert(id);
                }
                Command::ReleaseCancel(id) => {
                    let mut shared = shared.lock().unwrap_or_else(|e| e.into_inner());
                    shared.cancel_requested.remove(&id);
                    shared.cancel_processed.remove(&id);
                }
                Command::Wake => {
                    for rule in rules.iter().filter(|rule| {
                        rule.enabled && rule.last_outcome == Some(SyncRuleOutcome::Waiting)
                    }) {
                        queue(&mut due, &mut causes, &rule.id, SyncRuleRunCause::Interval);
                    }
                }
            },
            Some(id) = changes.recv() => {
                changed_at.insert(id, Instant::now());
            }
            Some(joined) = runs.join_next(), if !runs.is_empty() => {
                if let Ok((id, report)) = joined {
                    cancels.remove(&id);
                    {
                        let mut shared = shared.lock().unwrap_or_else(|e| e.into_inner());
                        shared.running.remove(&id);
                        shared.last.insert(id.clone(), report);
                    }
                    // The rule row changed with the result.
                    rules = runner.rules().await;
                    runner.changed(Some(id));
                }
            }
            _ = tick.tick() => {}
        }
        let now = Instant::now();
        let quiet: Vec<String> = changed_at
            .iter()
            .filter(|(_, at)| now.duration_since(**at) >= QUIET)
            .map(|(id, _)| id.clone())
            .collect();
        for id in quiet {
            changed_at.remove(&id);
            queue(&mut due, &mut causes, &id, SyncRuleRunCause::Watch);
        }
        for rule in rules.iter().filter(|rule| rule.enabled) {
            let late = last_start
                .get(&rule.id)
                .is_some_and(|at| now.duration_since(*at) >= interval_of(rule));
            if late && !cancels.contains_key(&rule.id) {
                queue(&mut due, &mut causes, &rule.id, SyncRuleRunCause::Interval);
            }
        }
        let mut waiting = Vec::new();
        for id in due.drain(..) {
            if shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .cancel_requested
                .contains(&id)
            {
                causes.remove(&id);
                continue;
            }
            let Some(rule) = rules.iter().find(|rule| rule.id == id && rule.enabled) else {
                causes.remove(&id);
                continue;
            };
            if cancels.contains_key(&id) || runs.len() >= MAX_RUNS {
                waiting.push(id);
                continue;
            }
            let cause = causes.remove(&id).unwrap_or(SyncRuleRunCause::Interval);
            let cancel = token.child_token();
            cancels.insert(id.clone(), cancel.clone());
            last_start.insert(id.clone(), now);
            shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .running
                .insert(id.clone());
            runner.changed(Some(id.clone()));
            let (runner, rule) = (Arc::clone(&runner), rule.clone());
            runs.spawn(async move {
                let id = rule.id.clone();
                let report = std::panic::AssertUnwindSafe(runner.run(rule, cause, cancel))
                    .catch_unwind()
                    .await
                    .unwrap_or_else(|_| {
                        log::warn!("file_sync: a run of {id} panicked");
                        RunReport::stopped(SyncRuleOutcome::Error, "rule run panicked")
                    });
                (id, report)
            });
        }
        due = waiting;
    }
    drop(watches);
    for cancel in cancels.values() {
        cancel.cancel();
    }
    while runs.join_next().await.is_some() {}
    shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .running
        .clear();
    shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .cancel_requested
        .clear();
    shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .cancel_processed
        .clear();
}

/// Makes `id` due for `cause`, unless it is due already.
fn queue(
    due: &mut Vec<String>,
    causes: &mut HashMap<String, SyncRuleRunCause>,
    id: &str,
    cause: SyncRuleRunCause,
) {
    if !due.iter().any(|other| other == id) {
        due.push(id.to_owned());
    }
    causes.entry(id.to_owned()).or_insert(cause);
}

fn starts_after_reload(before: Option<&SyncRule>, rule: &SyncRule) -> bool {
    before.is_some_and(|before| {
        !before.enabled
            || before.place_a != rule.place_a
            || before.place_b != rule.place_b
            || before.direction != rule.direction
    })
}

fn watch_all<R: Runner>(
    runner: &R,
    rules: &[SyncRule],
    changes: &tokio::sync::mpsc::UnboundedSender<String>,
) -> Vec<Box<dyn Send>> {
    rules
        .iter()
        .filter(|rule| rule.enabled)
        .filter_map(|rule| {
            let (changes, id) = (changes.clone(), rule.id.clone());
            runner.watch(
                rule,
                Arc::new(move || {
                    let _ = changes.send(id.clone());
                }),
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "schedule_tests.rs"]
mod tests;

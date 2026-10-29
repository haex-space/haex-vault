//! The control loop with a fake runner and a paused clock (no sleep): catch-up after the open, the
//! interval, a quiet watched place, "Jetzt abgleichen", one run per rule, at most two at once, a
//! waiting rule woken by the devices, and the end of the session.

use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::Notify;

use super::*;
use crate::file_sync::execute::Counts;
use crate::file_sync::model::{
    SyncRuleDeleteMode, SyncRuleDirection, SyncRuleOutcome, SyncRulePlace,
};

fn rule(id: &str, enabled: bool, remote: bool) -> SyncRule {
    let place = |path: &str| SyncRulePlace {
        source: SourceRef::Device,
        path: path.to_owned(),
    };
    let mut b = place("/b");
    if remote {
        b.source = SourceRef::Storage {
            storage_id: "s3".to_owned(),
        };
    }
    SyncRule {
        id: id.to_owned(),
        name: id.to_owned(),
        place_a: place("/a"),
        place_b: b,
        direction: SyncRuleDirection::AToB,
        delete_mode: SyncRuleDeleteMode::Keep,
        interval_secs: 60,
        enabled,
        allow_decrypted: false,
        created_at: String::new(),
        updated_at: String::new(),
        last_run_at: None,
        last_outcome: None,
        last_error: None,
    }
}

fn report() -> RunReport {
    RunReport {
        outcome: SyncRuleOutcome::Done,
        error: None,
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

#[derive(Default)]
struct Fake {
    rules: Mutex<Vec<SyncRule>>,
    runs: Mutex<Vec<String>>,
    causes: Mutex<Vec<SyncRuleRunCause>>,
    active: AtomicUsize,
    most: AtomicUsize,
    /// Runs wait here when set, until notified.
    gate: Mutex<Option<Arc<Notify>>>,
    notify: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

#[async_trait]
impl Runner for Fake {
    async fn rules(&self) -> Vec<SyncRule> {
        self.rules.lock().unwrap().clone()
    }
    async fn run(
        &self,
        rule: SyncRule,
        cause: SyncRuleRunCause,
        cancel: CancellationToken,
    ) -> RunReport {
        self.runs.lock().unwrap().push(rule.id);
        self.causes.lock().unwrap().push(cause);
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        let gate = self.gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            tokio::select! {
                () = gate.notified() => {}
                () = cancel.cancelled() => {}
            }
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
        report()
    }
    fn changed(&self, _rule_id: Option<String>) {}
    fn watch(&self, rule: &SyncRule, notify: Arc<dyn Fn() + Send + Sync>) -> Option<Box<dyn Send>> {
        if rule.id == "watched" {
            *self.notify.lock().unwrap() = Some(notify);
            return Some(Box::new(()));
        }
        None
    }
}

struct Loop {
    fake: Arc<Fake>,
    shared: Arc<Mutex<Shared>>,
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    token: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

fn start(rules: Vec<SyncRule>) -> Loop {
    let fake = Arc::new(Fake::default());
    *fake.rules.lock().unwrap() = rules;
    let (commands, received) = unbounded_channel();
    let token = CancellationToken::new();
    let shared = Arc::new(Mutex::new(Shared::default()));
    let task = tokio::spawn(control(
        Arc::clone(&fake),
        received,
        Arc::clone(&shared),
        token.clone(),
    ));
    Loop {
        fake,
        shared,
        commands,
        token,
        task,
    }
}

#[tokio::test(start_paused = true)]
async fn a_cancel_requested_before_the_command_reaches_the_loop_prevents_a_run() {
    let l = start(vec![rule("a", true, true)]);
    l.shared
        .lock()
        .unwrap()
        .cancel_requested
        .insert("a".to_owned());
    l.commands.send(Command::Cancel("a".to_owned())).unwrap();
    settle(TICK).await;
    assert!(runs(&l).is_empty());
    l.commands
        .send(Command::ReleaseCancel("a".to_owned()))
        .unwrap();
    settle(TICK).await;
    l.token.cancel();
    l.task.await.unwrap();
}

async fn settle(by: Duration) {
    tokio::time::advance(by).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

fn runs(l: &Loop) -> Vec<String> {
    l.fake.runs.lock().unwrap().clone()
}

#[tokio::test(start_paused = true)]
async fn active_rules_catch_up_after_the_open_and_paused_ones_do_not() {
    let l = start(vec![rule("a", true, true), rule("off", false, true)]);
    settle(TICK).await;
    assert_eq!(runs(&l), ["a"]);
    l.token.cancel();
    l.task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_remote_rule_runs_again_after_its_interval() {
    let l = start(vec![rule("a", true, true)]);
    settle(TICK).await;
    settle(Duration::from_secs(30)).await;
    assert_eq!(runs(&l).len(), 1);
    settle(Duration::from_secs(31)).await;
    assert_eq!(runs(&l).len(), 2);
    assert_eq!(l.fake.causes.lock().unwrap()[1], SyncRuleRunCause::Interval);
    l.token.cancel();
    l.task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_local_rule_waits_for_the_safety_interval_or_a_quiet_change() {
    let l = start(vec![rule("watched", true, false)]);
    settle(TICK).await;
    settle(Duration::from_secs(120)).await;
    assert_eq!(runs(&l).len(), 1, "no interval for local places");
    let notify = l.fake.notify.lock().unwrap().clone().unwrap();
    notify();
    settle(Duration::from_secs(1)).await;
    assert_eq!(runs(&l).len(), 1, "not before it is quiet");
    settle(QUIET).await;
    assert_eq!(runs(&l).len(), 2);
    assert_eq!(
        *l.fake.causes.lock().unwrap(),
        [SyncRuleRunCause::Startup, SyncRuleRunCause::Watch]
    );
    l.token.cancel();
    l.task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn changes_reach_a_rule_with_a_remote_place_no_faster_than_its_interval() {
    let l = start(vec![rule("watched", true, true)]);
    settle(TICK).await;
    assert_eq!(runs(&l).len(), 1);
    let notify = l.fake.notify.lock().unwrap().clone().unwrap();
    notify();
    settle(QUIET + TICK).await;
    assert_eq!(runs(&l).len(), 1, "quiet, but the interval has not passed");
    // The run at the interval takes the change with it; a change after it waits again.
    settle(Duration::from_secs(60)).await;
    assert_eq!(runs(&l).len(), 2);
    notify();
    settle(QUIET + TICK).await;
    assert_eq!(runs(&l).len(), 2);
    settle(Duration::from_secs(60)).await;
    assert_eq!(runs(&l).len(), 3);
    l.token.cancel();
    l.task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn run_now_runs_and_a_rule_runs_once_at_a_time() {
    let l = start(vec![rule("a", true, true)]);
    let gate = Arc::new(Notify::new());
    *l.fake.gate.lock().unwrap() = Some(Arc::clone(&gate));
    settle(TICK).await;
    l.commands.send(Command::Run("a".to_owned())).unwrap();
    settle(TICK).await;
    assert_eq!(runs(&l).len(), 1, "the second waits for the first");
    gate.notify_waiters();
    settle(TICK).await;
    settle(TICK).await;
    assert_eq!(runs(&l).len(), 2);
    assert_eq!(l.fake.causes.lock().unwrap()[1], SyncRuleRunCause::Manual);
    gate.notify_waiters();
    l.token.cancel();
    l.task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn at_most_two_rules_run_at_once_and_the_end_cancels_runs() {
    let l = start(vec![
        rule("a", true, true),
        rule("b", true, true),
        rule("c", true, true),
    ]);
    let gate = Arc::new(Notify::new());
    *l.fake.gate.lock().unwrap() = Some(Arc::clone(&gate));
    settle(TICK).await;
    assert_eq!(runs(&l).len(), 2);
    assert_eq!(l.fake.most.load(Ordering::SeqCst), MAX_RUNS);
    l.token.cancel();
    l.task.await.unwrap();
    assert_eq!(
        l.fake.active.load(Ordering::SeqCst),
        0,
        "runs ended with the session"
    );
}

#[tokio::test(start_paused = true)]
async fn a_reload_runs_a_new_rule_once() {
    let l = start(vec![]);
    settle(TICK).await;
    l.fake.rules.lock().unwrap().push(rule("new", true, true));
    l.commands.send(Command::Reload).unwrap();
    settle(TICK).await;
    assert_eq!(runs(&l), ["new"]);
    l.token.cancel();
    l.task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_reload_runs_a_rule_that_was_enabled_or_reconfigured() {
    let l = start(vec![rule("a", false, true)]);
    settle(TICK).await;
    assert!(runs(&l).is_empty());

    l.fake.rules.lock().unwrap()[0].enabled = true;
    l.commands.send(Command::Reload).unwrap();
    settle(TICK).await;
    assert_eq!(runs(&l), ["a"]);

    l.fake.rules.lock().unwrap()[0].place_b.path = "/new".to_owned();
    l.commands.send(Command::Reload).unwrap();
    settle(TICK).await;
    assert_eq!(runs(&l), ["a", "a"]);

    l.token.cancel();
    l.task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_change_of_the_devices_wakes_only_waiting_rules() {
    let mut waiting = rule("waiting", true, true);
    waiting.last_outcome = Some(SyncRuleOutcome::Waiting);
    let l = start(vec![waiting, rule("done", true, true)]);
    settle(TICK).await;
    assert_eq!(runs(&l).len(), 2, "both catch up");
    // The fake's rows keep saying "waiting" for the first rule.
    l.commands.send(Command::Wake).unwrap();
    settle(TICK).await;
    let count = |id: &str| runs(&l).iter().filter(|run| *run == id).count();
    assert_eq!((count("waiting"), count("done")), (2, 1));
    l.token.cancel();
    l.task.await.unwrap();
}

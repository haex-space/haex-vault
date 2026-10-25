//! Tests for a local model as the model of a job (spec 055 T076, T079): the model is borrowed for
//! the request and given back when the answer is complete or the job lets go of the stream, the
//! wait for the place counts towards the job's time, and a slot that cannot lend makes the model
//! `unavailable` without telling why.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::candidates::Candidate;
use super::effort::Level;
use super::errors::JobErrorCode;
use super::job::{self, Step};
use super::local_slot::{Loan, LocalSlot};
use super::model_source::{JobModel, ModelSource, Providers};
use super::runner::{run_step, Outcome, StepEnd, StepRun};
use super::test_support::{model_row, provider, FakeSource, Script};
use crate::adapters::ProviderAdapter;
use crate::storage::providers::ProviderKind;

const GOOD: &str = r#"{"texts":["Das ist ein Test."]}"#;

/// What the slot does when it is asked.
enum Way {
    /// Lends the scripted model.
    Lend(Arc<dyn ProviderAdapter>),
    /// The chat's turn does not end: the model is never lent.
    Wait,
    /// Cannot lend, with this message.
    Fail(String),
}

struct ScriptedSlot {
    way: Way,
    lent: Mutex<Vec<String>>,
    released: Arc<AtomicUsize>,
}

impl ScriptedSlot {
    fn new(way: Way) -> Arc<Self> {
        Arc::new(Self {
            way,
            lent: Mutex::new(Vec::new()),
            released: Arc::new(AtomicUsize::new(0)),
        })
    }
}

#[async_trait]
impl LocalSlot for ScriptedSlot {
    async fn lend(&self, model_id: &str) -> Result<Loan, String> {
        self.lent.lock().unwrap().push(model_id.to_owned());
        match &self.way {
            Way::Lend(adapter) => {
                let released = Arc::clone(&self.released);
                Ok(Loan::until(Arc::clone(adapter), move || {
                    released.fetch_add(1, Ordering::SeqCst);
                }))
            }
            Way::Wait => std::future::pending().await,
            Way::Fail(message) => Err(message.clone()),
        }
    }
}

fn local_model() -> Candidate {
    let installed = provider(ProviderKind::Local, None, None);
    Candidate {
        model: JobModel::new(model_row(&installed, "qwen3-4b", None), installed),
        ceiling: Level::None,
    }
}

fn step() -> Step {
    job::parse(&json!({ "steps": [{
        "id": "fix",
        "needs": ["text"],
        "instruction": "Korrigiere nur Rechtschreibung.",
        "input": { "text": "Das ist ein Tsest." },
        "format": { "name": "text-fix", "version": 1, "schema": {
            "type": "object",
            "required": ["texts"],
            "properties": { "texts": { "type": "array", "items": { "type": "string" } } }
        }}
    }]}))
    .unwrap()
    .steps
    .remove(0)
}

/// The scripted model behind the slot: whatever `script` says, for the model of `candidate`.
fn model_behind(candidate: &Candidate, script: Script) -> Arc<dyn ProviderAdapter> {
    FakeSource::with(&[(&candidate.model.id, script)])
        .adapter(&candidate.model)
        .expect("scripted adapter")
}

async fn run(
    slot: Arc<ScriptedSlot>,
    candidate: &Candidate,
    cancel: &CancellationToken,
    within: Duration,
) -> StepRun {
    let source = Providers::new(Default::default(), Some(slot));
    run_step(
        &source,
        &step(),
        std::slice::from_ref(candidate),
        cancel,
        Instant::now() + within,
        None,
    )
    .await
}

async fn until(released: &AtomicUsize, expected: usize) {
    for _ in 0..200 {
        if released.load(Ordering::SeqCst) == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("released {} of {expected}", released.load(Ordering::SeqCst));
}

#[tokio::test]
async fn a_local_model_answers_a_job_and_the_place_is_given_back_once() {
    let candidate = local_model();
    let slot = ScriptedSlot::new(Way::Lend(model_behind(
        &candidate,
        Script::Answer(GOOD.into()),
    )));

    let run = run(
        Arc::clone(&slot),
        &candidate,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await;

    assert!(matches!(run.end, StepEnd::Used(_)), "{:?}", run.end);
    assert_eq!(
        *slot.lent.lock().unwrap(),
        std::slice::from_ref(&candidate.model.id)
    );
    until(&slot.released, 1).await;
}

#[tokio::test]
async fn a_cancelled_job_gives_the_place_back() {
    let candidate = local_model();
    let slot = ScriptedSlot::new(Way::Lend(model_behind(&candidate, Script::Hang)));
    let cancel = CancellationToken::new();
    let stopper = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        stopper.cancel();
    });

    let run = run(
        Arc::clone(&slot),
        &candidate,
        &cancel,
        Duration::from_secs(60),
    )
    .await;

    assert_eq!(run.end, StepEnd::Cancelled);
    until(&slot.released, 1).await;
}

#[tokio::test]
async fn the_wait_for_the_place_counts_towards_the_time_of_the_job() {
    let candidate = local_model();
    let slot = ScriptedSlot::new(Way::Wait);

    let run = run(
        Arc::clone(&slot),
        &candidate,
        &CancellationToken::new(),
        Duration::from_millis(100),
    )
    .await;

    assert_eq!(run.attempts[0].outcome, Outcome::Timeout);
    assert!(matches!(run.end, StepEnd::Failed(JobErrorCode::Timeout, _)));
    assert_eq!(slot.released.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_slot_that_cannot_lend_makes_the_model_unavailable_without_saying_why() {
    let candidate = local_model();
    let slot = ScriptedSlot::new(Way::Fail("hash of /home/ada/models/q.gguf differs".into()));

    let run = run(
        slot,
        &candidate,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await;

    assert_eq!(run.attempts[0].outcome, Outcome::Unavailable);
    let reason = run.attempts[0].reason.clone().unwrap_or_default();
    assert!(!reason.contains("/home/ada"), "{reason}");
    assert!(matches!(
        run.end,
        StepEnd::Failed(JobErrorCode::Unavailable, _)
    ));
}

#[test]
fn a_build_without_a_slot_has_no_local_models() {
    let candidate = local_model();
    let message = Providers::default()
        .adapter(&candidate.model)
        .err()
        .expect("no adapter");
    assert!(message.contains("not available"), "{message}");
}

#[test]
fn a_pending_loan_releases_the_place_when_loading_is_cancelled() {
    let released = Arc::new(AtomicUsize::new(0));
    {
        let released = Arc::clone(&released);
        let _loan = Loan::pending(move || {
            released.fetch_add(1, Ordering::SeqCst);
        });
    }
    assert_eq!(released.load(Ordering::SeqCst), 1);
}

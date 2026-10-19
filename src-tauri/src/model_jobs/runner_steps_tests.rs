//! Tests for a job of several steps (spec 055 T040): the result of a step reaches a later one
//! through `from` and its pointer, a part missing at run time ends the step without a model, the
//! first step without a result ends the job, and a grant withdrawn while the job runs holds from
//! the next step on.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use serde_json::{json, Value};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::candidates::Candidate;
use super::effort::Level;
use super::errors::JobErrorCode;
use super::job::{self, Step};
use super::model_source::JobModel;
use super::runner::{run_job, JobEnd, JobRun, Outcome, Recheck};
use super::test_support::{model_row, provider, FakeSource, Script};
use crate::storage::providers::ProviderKind;

fn candidate(name: &str) -> Candidate {
    let server = provider(
        ProviderKind::ApiKey,
        Some("http://localhost:11434/v1"),
        None,
    );
    Candidate {
        model: JobModel::new(model_row(&server, name, None), server),
        ceiling: Level::None,
    }
}

/// The two steps of the contract's example: read regions, then fix their texts.
fn steps(regions_required: bool) -> Vec<Step> {
    let required = if regions_required {
        json!(["regions"])
    } else {
        json!([])
    };
    let job = json!({ "steps": [
        {
            "id": "read",
            "needs": ["text"],
            "instruction": "Zerlege die Seite in Regionen.",
            "input": { "text": "Seite 3" },
            "format": { "name": "page-recognition", "version": 1, "schema": {
                "type": "object",
                "required": required,
                "properties": { "regions": { "type": "array", "items": {
                    "type": "object", "properties": { "text": { "type": "string" } }
                }}}
            }}
        },
        {
            "id": "fix",
            "needs": ["text"],
            "instruction": "Korrigiere nur Rechtschreibung.",
            "input": { "from": { "step": "read", "pointer": "/regions" } },
            "format": { "name": "text-fix", "version": 1, "schema": {
                "type": "object",
                "required": ["texts"],
                "properties": { "texts": { "type": "array", "items": { "type": "string" } } }
            }}
        }
    ]});
    job::parse(&job).unwrap().steps
}

/// The grants as they are between two steps: every planned candidate, except the `withdrawn`
/// ones; `None` for grants that cannot be read.
#[derive(Default)]
struct Grants {
    withdrawn: Vec<String>,
    unreadable: bool,
    asked: AtomicUsize,
}

#[async_trait]
impl Recheck for Grants {
    async fn still_allowed(&self, planned: &[Candidate]) -> Option<Vec<Candidate>> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        (!self.unreadable).then(|| {
            planned
                .iter()
                .filter(|c| !self.withdrawn.contains(&c.model.id))
                .cloned()
                .collect()
        })
    }
}

async fn run_with(
    source: &FakeSource,
    steps: Vec<Step>,
    models: [&Candidate; 2],
    grants: &Grants,
) -> JobRun {
    let planned: Vec<(Step, Vec<Candidate>)> = steps
        .into_iter()
        .zip(models)
        .map(|(step, model)| (step, vec![model.clone()]))
        .collect();
    let deadline = Instant::now() + Duration::from_secs(60);
    run_job(
        source,
        &planned,
        grants,
        &CancellationToken::new(),
        deadline,
    )
    .await
}

async fn run(source: &FakeSource, steps: Vec<Step>, models: [&Candidate; 2]) -> JobRun {
    run_with(source, steps, models, &Grants::default()).await
}

fn ids(run: &JobRun) -> Vec<(String, usize)> {
    run.steps
        .iter()
        .map(|step| (step.step_id.clone(), step.attempts.len()))
        .collect()
}

#[tokio::test]
async fn the_result_of_a_step_reaches_the_next_through_from_and_its_pointer() {
    let (reader, fixer) = (candidate("reader"), candidate("fixer"));
    let read = json!({ "regions": [{ "text": "Das ist ein Tsest." }] });
    let fixed = json!({ "texts": ["Das ist ein Test."] });
    let source = FakeSource::with(&[
        (&reader.model.id, Script::Answer(read.to_string())),
        (&fixer.model.id, Script::Answer(fixed.to_string())),
    ]);

    let run = run(&source, steps(true), [&reader, &fixer]).await;

    assert_eq!(
        run.end,
        JobEnd::Completed(vec![("read".into(), read), ("fix".into(), fixed)])
    );
    assert_eq!(ids(&run), [("read".into(), 1), ("fix".into(), 1)]);
    assert_eq!(run.steps[1].format_name, "text-fix");
    assert_eq!(run.steps[1].format_version, 1);
    let seen = source.seen.lock().unwrap();
    let fix_text = &seen[1].1.messages[0].content;
    // Only the part under the pointer goes on, as JSON: not the object around it.
    assert!(
        fix_text.starts_with(r#"[{"text":"Das ist ein Tsest."}]"#),
        "{fix_text}"
    );
    assert!(!fix_text.contains("regions"), "{fix_text}");
    assert_eq!(
        seen[1].1.system_prompt.as_deref(),
        Some("Korrigiere nur Rechtschreibung.")
    );
}

#[tokio::test]
async fn a_part_missing_at_run_time_ends_the_step_unusable_without_asking_a_model() {
    let (reader, fixer) = (candidate("reader"), candidate("fixer"));
    let source = FakeSource::with(&[
        // Valid for the format, which does not require `regions`.
        (&reader.model.id, Script::Answer("{}".into())),
        (&fixer.model.id, Script::Answer(r#"{"texts":[]}"#.into())),
    ]);

    let run = run(&source, steps(false), [&reader, &fixer]).await;

    assert_eq!(
        run.end,
        JobEnd::Failed {
            step_id: "fix".into(),
            code: JobErrorCode::Unusable,
            message: "from missing".into(),
        }
    );
    // Another model cannot change what the first step gave: the step lists no attempt.
    assert_eq!(ids(&run), [("read".into(), 1), ("fix".into(), 0)]);
    assert_eq!(source.asked(), std::slice::from_ref(&reader.model.id));
}

#[tokio::test]
async fn the_first_step_without_a_result_ends_the_job() {
    let (reader, fixer) = (candidate("reader"), candidate("fixer"));
    let source = FakeSource::with(&[
        (&reader.model.id, Script::Answer("".into())),
        (&fixer.model.id, Script::Answer(r#"{"texts":[]}"#.into())),
    ]);

    let run = run(&source, steps(true), [&reader, &fixer]).await;

    assert!(matches!(
        &run.end,
        JobEnd::Failed { step_id, code: JobErrorCode::Unusable, .. } if step_id == "read"
    ));
    assert_eq!(ids(&run), [("read".into(), 1)]);
    assert_eq!(run.steps[0].attempts[0].outcome, Outcome::Unusable);
    assert_eq!(source.asked(), std::slice::from_ref(&reader.model.id));
}

#[tokio::test]
async fn the_script_of_a_handed_part_may_come_back_in_the_answer() {
    let (reader, fixer) = (candidate("reader"), candidate("fixer"));
    let greek: Value = json!({ "regions": [{ "text": "λόγος και μύθος" }] });
    let fixed = json!({ "texts": ["λόγος καὶ μῦθος"] });
    let source = FakeSource::with(&[
        (&reader.model.id, Script::Answer(greek.to_string())),
        (&fixer.model.id, Script::Answer(fixed.to_string())),
    ]);
    let mut planned = steps(true);
    // The first step's own input names Greek, so its answer may hold it too.
    planned[0].input.text = Some("Seite 3, Griechisch: λόγος".into());

    let run = run(&source, planned, [&reader, &fixer]).await;

    // Greek in the second step's answer is no script switch: it was handed over in Greek.
    assert_eq!(
        run.end,
        JobEnd::Completed(vec![("read".into(), greek), ("fix".into(), fixed)])
    );
}

fn two_answers(reader: &Candidate, fixer: &Candidate) -> std::sync::Arc<FakeSource> {
    FakeSource::with(&[
        (
            &reader.model.id,
            Script::Answer(json!({ "regions": [{ "text": "Tsest" }] }).to_string()),
        ),
        (
            &fixer.model.id,
            Script::Answer(r#"{"texts":["Test"]}"#.into()),
        ),
    ])
}

#[tokio::test]
async fn the_grants_are_asked_again_before_every_later_step_only() {
    let (reader, fixer) = (candidate("reader"), candidate("fixer"));
    let source = two_answers(&reader, &fixer);
    let grants = Grants::default();

    let run = run_with(&source, steps(true), [&reader, &fixer], &grants).await;

    assert!(matches!(run.end, JobEnd::Completed(_)));
    // The first step runs on what the start checked a moment ago.
    assert_eq!(grants.asked.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_grant_withdrawn_while_the_job_runs_ends_the_next_step() {
    let (reader, fixer) = (candidate("reader"), candidate("fixer"));
    let source = two_answers(&reader, &fixer);
    let grants = Grants {
        withdrawn: vec![fixer.model.id.clone()],
        ..Grants::default()
    };

    let run = run_with(&source, steps(true), [&reader, &fixer], &grants).await;

    assert_eq!(
        run.end,
        JobEnd::Failed {
            step_id: "fix".into(),
            code: JobErrorCode::NoCapableModel,
            message: "a grant was withdrawn".into(),
        }
    );
    assert_eq!(ids(&run), [("read".into(), 1), ("fix".into(), 0)]);
    assert_eq!(source.asked(), std::slice::from_ref(&reader.model.id));
}

#[tokio::test]
async fn grants_that_cannot_be_read_end_the_next_step_unavailable() {
    let (reader, fixer) = (candidate("reader"), candidate("fixer"));
    let source = two_answers(&reader, &fixer);
    let grants = Grants {
        unreadable: true,
        ..Grants::default()
    };

    let run = run_with(&source, steps(true), [&reader, &fixer], &grants).await;

    assert!(matches!(
        &run.end,
        JobEnd::Failed { step_id, code: JobErrorCode::Unavailable, .. } if step_id == "fix"
    ));
    assert_eq!(source.asked(), std::slice::from_ref(&reader.model.id));
}

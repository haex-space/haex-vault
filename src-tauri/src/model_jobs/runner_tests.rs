//! Tests for the run of one step (spec 055 T026, T038, T042, T044): the request, the guard and the
//! format per try, the fallback over the candidates, the deadline and the cancel token.

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::candidates::Candidate;
use super::effort::Level;
use super::job::{self, Step};
use super::model_source::{JobModel, ModelSource, Providers};
use super::runner::{run_step, Outcome, StepEnd, StepRun};
use super::test_support::{model_row, provider, FakeSource, Script};
use crate::adapters::AttachmentKind;
use crate::model_capabilities::{ModelCapabilities, ReasoningControl, ReasoningOption};
use crate::storage::providers::ProviderKind;

const KEY: &str = "sk-school-0123456789abcdef";
const ADDRESS: &str = "http://10.0.0.5:8080/v1";

fn step(extra: Value) -> Step {
    let mut one = json!({
        "id": "fix",
        "needs": ["text"],
        "instruction": "Korrigiere nur Rechtschreibung.",
        "input": { "text": "Das ist ein Tsest." },
        "format": { "name": "text-fix", "version": 1, "schema": {
            "type": "object",
            "required": ["texts"],
            "properties": { "texts": { "type": "array", "items": { "type": "string" } } }
        }}
    });
    if let (Some(one), Some(extra)) = (one.as_object_mut(), extra.as_object()) {
        one.extend(extra.clone());
    }
    job::parse(&json!({ "steps": [one] }))
        .unwrap()
        .steps
        .remove(0)
}

fn candidate(name: &str, ceiling: Level, capabilities: Option<ModelCapabilities>) -> Candidate {
    let server = provider(ProviderKind::ApiKey, Some(ADDRESS), Some(KEY));
    Candidate {
        model: JobModel::new(model_row(&server, name, capabilities), server),
        ceiling,
    }
}

async fn run(source: &dyn ModelSource, step: &Step, candidates: &[Candidate]) -> StepRun {
    let deadline = Instant::now() + Duration::from_secs(60);
    run_step(
        source,
        step,
        candidates,
        &CancellationToken::new(),
        deadline,
    )
    .await
}

fn outcomes(run: &StepRun) -> Vec<(String, Outcome, Option<String>)> {
    run.attempts
        .iter()
        .map(|a| (a.model.clone(), a.outcome, a.reason.clone()))
        .collect()
}

const GOOD: &str = r#"{"texts":["Das ist ein Test."]}"#;

#[tokio::test]
async fn the_request_carries_no_tool_the_instruction_the_images_and_the_schema() {
    let levels = ModelCapabilities {
        reasoning: Some(ReasoningControl::Presets {
            options: ["low", "medium", "high"]
                .iter()
                .map(|id| ReasoningOption {
                    id: (*id).into(),
                    label: (*id).into(),
                })
                .collect(),
        }),
        accepted_attachment_kinds: Some(vec![AttachmentKind::Image]),
        ..Default::default()
    };
    let model = candidate("qwen3-vl", Level::Medium, Some(levels));
    let source = FakeSource::with(&[(&model.model.id, Script::Answer(GOOD.into()))]);
    let read = step(json!({
        "needs": ["text", "image"],
        "input": { "text": "Seite 3", "images": [{ "mimeType": "image/png", "data": "iVBORw0KGgo=" }] },
        "effort": "high",
        "maxOutputTokens": 8192,
    }));

    let run = run(source.as_ref(), &read, std::slice::from_ref(&model)).await;

    assert_eq!(
        run.end,
        StepEnd::Used(json!({ "texts": ["Das ist ein Test."] }))
    );
    let seen = source.seen.lock().unwrap();
    let request = &seen[0].1;
    assert!(request.tools.is_empty(), "no tools, ever (FR-026)");
    assert_eq!(model.model.remote_id, "qwen3-vl");
    assert_eq!(request.model_id, "qwen3-vl");
    assert_eq!(
        request.system_prompt.as_deref(),
        Some("Korrigiere nur Rechtschreibung.")
    );
    assert_eq!(request.max_new_tokens, Some(8192));
    let message = &request.messages[0];
    assert!(message.content.starts_with("Seite 3\n\n"));
    assert!(message.content.contains("valid JSON only"));
    assert!(message.content.contains(r#""required":["texts"]"#));
    assert_eq!(message.attachments.len(), 1);
    assert_eq!(message.attachments[0].kind, AttachmentKind::Image);
    assert_eq!(message.attachments[0].media_type, "image/png");
    assert_eq!(message.attachments[0].bytes[..4], [0x89, b'P', b'N', b'G']);
    // The wish `high` is capped at the ceiling `medium`, and the provenance says so (FR-006).
    assert_eq!(request.reasoning_option.as_deref(), Some("medium"));
    assert!(request.reasoning_requested);
    assert_eq!(run.attempts[0].effort, "medium");
    assert_eq!(run.attempts[0].outcome, Outcome::Used);
}

#[tokio::test]
async fn broken_output_is_dropped_and_the_next_model_is_used_each_one_once() {
    let models = ["empty", "prose", "wrong", "loop", "good", "never"]
        .map(|name| candidate(name, Level::None, None));
    let looping = format!(r#"{{"texts":["{}"]}}"#, "1.00 Uhr ".repeat(40));
    let source = FakeSource::with(&[
        (&models[0].model.id, Script::Answer("  ".into())),
        (
            &models[1].model.id,
            Script::Answer("Hier ist der Text.".into()),
        ),
        (
            &models[2].model.id,
            Script::Answer(r#"{"texts":[1]}"#.into()),
        ),
        (&models[3].model.id, Script::Answer(looping)),
        (&models[4].model.id, Script::Answer(GOOD.into())),
        (&models[5].model.id, Script::Answer(GOOD.into())),
    ]);

    let run = run(source.as_ref(), &step(json!({})), &models).await;

    assert_eq!(
        run.end,
        StepEnd::Used(json!({ "texts": ["Das ist ein Test."] }))
    );
    let reasons: Vec<_> = outcomes(&run)
        .into_iter()
        .map(|(model, outcome, reason)| (model, outcome, reason.unwrap_or_default()))
        .collect();
    assert_eq!(
        reasons[0],
        ("empty".into(), Outcome::Unusable, "empty".into())
    );
    assert_eq!(
        reasons[1],
        ("prose".into(), Outcome::Unusable, "invalid json".into())
    );
    assert_eq!(reasons[2].1, Outcome::Unusable);
    assert!(reasons[2].2.starts_with("schema:"), "{:?}", reasons[2]);
    assert_eq!(
        reasons[3],
        ("loop".into(), Outcome::Unusable, "loop".into())
    );
    assert_eq!(reasons[4], ("good".into(), Outcome::Used, String::new()));
    assert_eq!(reasons.len(), 5);
    assert_eq!(
        source.asked(),
        models[..5]
            .iter()
            .map(|m| m.model.id.clone())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn when_every_model_fails_the_step_ends_with_the_last_reason() {
    let models = ["broken", "gone"].map(|name| candidate(name, Level::None, None));
    let source = FakeSource::with(&[
        (&models[0].model.id, Script::Answer("".into())),
        (
            &models[1].model.id,
            Script::Unbuildable("cannot build".into()),
        ),
    ]);

    let run = run(source.as_ref(), &step(json!({})), &models).await;

    assert_eq!(
        outcomes(&run),
        [
            ("broken".into(), Outcome::Unusable, Some("empty".into())),
            (
                "gone".into(),
                Outcome::Unavailable,
                Some("cannot build".into())
            ),
        ]
    );
    assert_eq!(
        run.end,
        StepEnd::Failed(
            super::errors::JobErrorCode::Unavailable,
            "cannot build".into()
        )
    );
}

#[tokio::test]
async fn a_tool_call_is_unusable_and_runs_nothing() {
    let model = candidate("agentic", Level::None, None);
    let source = FakeSource::with(&[(&model.model.id, Script::ToolCall)]);

    let run = run(source.as_ref(), &step(json!({})), &[model]).await;

    assert_eq!(
        outcomes(&run),
        [(
            "agentic".into(),
            Outcome::Unusable,
            Some("tool call".into())
        )]
    );
    assert!(matches!(
        run.end,
        StepEnd::Failed(super::errors::JobErrorCode::Unusable, _)
    ));
}

#[tokio::test]
async fn a_failure_never_names_the_key_the_address_or_the_body_of_the_answer() {
    let models = ["status", "stream"].map(|name| candidate(name, Level::None, None));
    let source = FakeSource::with(&[
        (
            &models[0].model.id,
            Script::Status(500, format!("echo of the prompt and {KEY}")),
        ),
        (
            &models[1].model.id,
            Script::StreamFails(format!("POST {ADDRESS}/chat/completions with {KEY} failed")),
        ),
    ]);

    let run = run(source.as_ref(), &step(json!({})), &models).await;

    let text = serde_json::to_string(&run.attempts).unwrap();
    for secret in [KEY, ADDRESS, "10.0.0.5", "echo of the prompt"] {
        assert!(!text.contains(secret), "{secret} in {text}");
    }
    assert_eq!(
        run.attempts[0].reason.as_deref(),
        Some("the provider answered with status 500")
    );
    let StepEnd::Failed(_, message) = &run.end else {
        panic!("{:?}", run.end);
    };
    assert!(!message.contains(KEY) && !message.contains("10.0.0.5"));
}

#[tokio::test(start_paused = true)]
async fn the_deadline_ends_the_try_and_drops_the_stream() {
    let models = ["slow", "next"].map(|name| candidate(name, Level::None, None));
    let source = FakeSource::with(&[
        (&models[0].model.id, Script::Hang),
        (&models[1].model.id, Script::Answer(GOOD.into())),
    ]);
    let deadline = Instant::now() + Duration::from_secs(300);

    let run = run_step(
        source.as_ref(),
        &step(json!({})),
        &models,
        &CancellationToken::new(),
        deadline,
    )
    .await;

    assert_eq!(outcomes(&run), [("slow".into(), Outcome::Timeout, None)]);
    assert!(matches!(
        run.end,
        StepEnd::Failed(super::errors::JobErrorCode::Timeout, _)
    ));
    assert!(Instant::now() >= deadline);
    // The job is over: the next model is not tried, and the hanging generation is aborted.
    assert_eq!(source.asked().len(), 1);
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert_eq!(source.dropped.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_job_stops_at_once_and_starts_no_request() {
    let models = ["slow", "next"].map(|name| candidate(name, Level::None, None));
    let source = FakeSource::with(&[
        (&models[0].model.id, Script::Hang),
        (&models[1].model.id, Script::Answer(GOOD.into())),
    ]);
    let cancel = CancellationToken::new();
    let later = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        later.cancel();
    });
    let deadline = Instant::now() + Duration::from_secs(300);

    let run = run_step(
        source.as_ref(),
        &step(json!({})),
        &models,
        &cancel,
        deadline,
    )
    .await;

    assert_eq!(run.end, StepEnd::Cancelled);
    assert_eq!(source.asked().len(), 1);
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert_eq!(source.dropped.load(Ordering::SeqCst), 1);

    // Cancelled before the start: nothing is asked at all.
    let fresh = FakeSource::with(&[(&models[1].model.id, Script::Answer(GOOD.into()))]);
    let run = run_step(
        fresh.as_ref(),
        &step(json!({})),
        &models[1..],
        &cancel,
        deadline,
    )
    .await;
    assert_eq!(run.end, StepEnd::Cancelled);
    assert!(fresh.asked().is_empty());
}

#[tokio::test]
async fn a_provider_that_cannot_be_reached_is_unavailable_without_its_address() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let address = format!("http://127.0.0.1:{port}/v1");
    let server = provider(ProviderKind::ApiKey, Some(&address), None);
    let model = Candidate {
        model: JobModel::new(model_row(&server, "m", None), server),
        ceiling: Level::None,
    };

    let run = run(&Providers, &step(json!({})), &[model]).await;

    assert_eq!(run.attempts[0].outcome, Outcome::Unavailable);
    let reason = run.attempts[0].reason.clone().unwrap_or_default();
    assert!(!reason.contains(&port.to_string()), "{reason}");
    assert!(matches!(
        run.end,
        StepEnd::Failed(super::errors::JobErrorCode::Unavailable, _)
    ));
}

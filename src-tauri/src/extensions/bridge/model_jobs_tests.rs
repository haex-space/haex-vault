//! Contract tests of the model-job methods (spec 055, `contracts/bridge.md`): they are on the
//! allowlist, the preflight answers with the fixed codes and a reason, and a job can be cancelled
//! by its own frame only.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use super::allowlist::METHODS;
use super::dispatch::call;
use super::dispatch::tests::setup;
use super::dispatch::CallContext;
use super::events::FRAME_EVENT;
use super::frames::FrameSession;
use crate::extensions::error::ExtensionErrorCode;
use crate::model_jobs::bridge::COMPLETED_EVENT;
use crate::model_jobs::JobOwner;

fn step() -> Value {
    json!({
        "id": "fix",
        "needs": ["text"],
        "instruction": "Korrigiere nur Rechtschreibung.",
        "input": { "text": "Das ist ein Tsest." },
        "format": { "name": "text-fix", "version": 1, "schema": {
            "type": "object",
            "properties": { "texts": { "type": "array", "items": { "type": "string" } } }
        }}
    })
}

fn job(steps: Vec<Value>) -> Value {
    json!({ "job": { "steps": steps } })
}

fn error_of(outcome: Result<Value, crate::extensions::error::BridgeError>) -> (u16, Value) {
    let error = outcome.unwrap_err();
    (error.code.as_u16(), error.details.unwrap_or(Value::Null))
}

fn owner_of(ctx: &CallContext) -> JobOwner {
    JobOwner {
        extension_id: ctx.session.extension_id,
        frame: ctx.session.frame.clone(),
    }
}

#[test]
fn the_model_methods_are_on_the_allowlist_and_the_task_methods_are_gone() {
    for name in [
        "extension_model_job_start",
        "extension_model_job_cancel",
        "extension_model_list",
    ] {
        let method = METHODS.iter().find(|m| m.name == name).expect(name);
        assert!(method.module.ends_with("model_jobs"), "{name}");
    }
    for name in ["extension_ai_task_start", "extension_ai_task_cancel"] {
        assert!(!METHODS.iter().any(|m| m.name == name), "{name}");
    }
}

#[test]
fn a_job_with_a_wrong_form_answers_3001_and_one_over_a_limit_7000() {
    let s = setup();
    let start = |params: Value| call(&s.ctx, "extension_model_job_start", &params);

    for params in [
        json!({}),
        json!({ "job": "nope" }),
        job(vec![]),
        job(vec![json!({ "id": "Fix", "needs": ["text"] })]),
        job(vec![step(), step()]),
        job(vec![{
            let mut bad = step();
            bad["format"]["schema"] = json!({ "type": "string", "pattern": "x" });
            bad
        }]),
    ] {
        let (code, details) = error_of(start(params.clone()));
        assert_eq!(code, 3001, "{params}");
        assert_eq!(details, json!({ "reason": "invalid_input" }));
    }

    let (code, details) = error_of(start(job((0..5)
        .map(|n| {
            let mut one = step();
            one["id"] = json!(format!("s{n}"));
            one
        })
        .collect())));
    assert_eq!(code, 7000);
    assert_eq!(details, json!({ "reason": "steps", "limit": "steps" }));

    let mut long = step();
    long["instruction"] = json!("x".repeat(16 * 1024 + 1));
    let (code, details) = error_of(start(job(vec![long])));
    assert_eq!(
        (code, details["reason"].clone()),
        (7000, json!("instruction_bytes"))
    );
}

#[test]
fn a_job_that_passes_the_preflight_is_not_available_before_the_runner_exists() {
    let s = setup();
    let (code, _) = error_of(call(
        &s.ctx,
        "extension_model_job_start",
        &job(vec![step()]),
    ));
    assert_eq!(code, ExtensionErrorCode::NotAvailable.as_u16());
    let (code, _) = error_of(call(&s.ctx, "extension_model_list", &json!({})));
    assert_eq!(code, ExtensionErrorCode::NotAvailable.as_u16());
    assert!(
        s.ctx.host.model_jobs().get(uuid::Uuid::nil()).is_none(),
        "nothing was registered"
    );
}

#[test]
fn an_unknown_job_is_not_cancelled_and_a_bad_id_is_refused() {
    let s = setup();
    let id = uuid::Uuid::nil().to_string();
    let answer = call(
        &s.ctx,
        "extension_model_job_cancel",
        &json!({ "jobId": id }),
    )
    .unwrap();
    assert_eq!(answer, json!({ "cancelled": false }));
    for params in [
        json!({}),
        json!({ "jobId": 5 }),
        json!({ "jobId": "not-a-uuid" }),
    ] {
        let (code, details) = error_of(call(&s.ctx, "extension_model_job_cancel", &params));
        assert_eq!(code, 3001, "{params}");
        assert_eq!(details, json!({ "reason": "invalid_input" }));
    }
    assert!(
        s.recorded.0.lock().unwrap().is_empty(),
        "no event for nothing"
    );
}

#[test]
fn a_job_is_cancelled_by_its_frame_and_the_extension_hears_it() {
    let s = setup();
    let record = s
        .ctx
        .host
        .model_jobs()
        .start(owner_of(&s.ctx), Duration::from_secs(60))
        .unwrap();

    let answer = call(
        &s.ctx,
        "extension_model_job_cancel",
        &json!({ "jobId": record.id.to_string() }),
    )
    .unwrap();

    assert_eq!(answer, json!({ "cancelled": true }));
    assert!(record.cancel.is_cancelled());
    assert!(
        s.ctx.host.model_jobs().get(record.id).is_none(),
        "nothing is kept"
    );
    let events = s.recorded.0.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    let (name, payload) = &events[0];
    assert_eq!(name, FRAME_EVENT);
    assert_eq!(payload["type"], COMPLETED_EVENT);
    assert_eq!(
        payload["data"],
        json!({
            "jobId": record.id.to_string(),
            "status": "cancelled",
            "provenance": [],
            "error": { "code": "cancelled", "message": "the job was cancelled" }
        })
    );

    // The second cancel finds nothing and says nothing.
    let again = call(
        &s.ctx,
        "extension_model_job_cancel",
        &json!({ "jobId": record.id.to_string() }),
    )
    .unwrap();
    assert_eq!(again, json!({ "cancelled": false }));
    assert_eq!(s.recorded.0.lock().unwrap().len(), 1);
}

#[test]
fn the_job_of_another_frame_is_not_cancelled_by_a_frame_of_the_same_extension() {
    let s = setup();
    let record = s
        .ctx
        .host
        .model_jobs()
        .start(owner_of(&s.ctx), Duration::from_secs(60))
        .unwrap();
    let bundle = match &s.ctx.session.source {
        crate::extensions::bridge::frames::FrameSource::Bundle(id) => *id,
        other => panic!("a bundle frame, not {other:?}"),
    };
    let other: Arc<FrameSession> =
        s.ctx
            .host
            .frames
            .open(s.ctx.session.extension_id, bundle, "tab-2");
    let intruder = CallContext {
        db: s.ctx.db.clone(),
        host: Arc::clone(&s.ctx.host),
        session: other,
        device: s.ctx.device,
        emitter: Arc::clone(&s.ctx.emitter),
    };

    let (code, _) = error_of(call(
        &intruder,
        "extension_model_job_cancel",
        &json!({ "jobId": record.id.to_string() }),
    ));

    assert_eq!(code, ExtensionErrorCode::SecurityViolation.as_u16());
    assert!(!record.cancel.is_cancelled());
    assert!(s.ctx.host.model_jobs().get(record.id).is_some());
    assert!(s.recorded.0.lock().unwrap().is_empty());
}

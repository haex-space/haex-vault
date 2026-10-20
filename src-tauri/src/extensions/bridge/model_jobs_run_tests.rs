//! A model job through the bridge (spec 055 T021, T028, T029, T045): a granted model answers and
//! the extension hears the result with its provenance; without a grant or a consent the user is
//! asked and no model is called; no answer or event carries a key or an address.

use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use super::dispatch::call;
use super::dispatch::tests::{setup, Setup};
use super::events::FRAME_EVENT;
use super::permissions::PERMISSION_REQUEST;
use crate::adapters::AttachmentKind;
use crate::chat::default_model::PREF_DEFAULT_MODEL;
use crate::extensions::error::BridgeError;
use crate::model_capabilities::{ModelCapabilities, ReasoningControl, ReasoningOption};
use crate::model_jobs::bridge::COMPLETED_EVENT;
use crate::model_jobs::test_support::{
    model_row, provider, remember, store_models, FakeSource, Script,
};
use crate::model_jobs::JobOwner;
use crate::storage::models::ModelRow;
use crate::storage::preferences::{insert_or_update, PrefScope};
use crate::storage::providers::{Provider, ProviderKind};

pub(super) const GOOD: &str = r#"{"texts":["Das ist ein Test."]}"#;
const KEY: &str = "sk-school-0123456789abcdef";
const PROVIDER_BODY: &str = "student answer body";

pub(super) fn step() -> Value {
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

pub(super) fn image_step() -> Value {
    let mut read = step();
    read["needs"] = json!(["text", "image"]);
    read["input"]["images"] = json!([{ "mimeType": "image/png", "data": "iVBORw0KGgo=" }]);
    read
}

pub(super) fn job(steps: Vec<Value>) -> Value {
    json!({ "job": { "steps": steps } })
}

/// Stores a provider at `address` with one model per name, and returns the model rows.
pub(super) fn models(
    s: &Setup,
    address: &str,
    names: &[&str],
    images: bool,
) -> (Provider, Vec<ModelRow>) {
    let server = provider(ProviderKind::ApiKey, Some(address), Some(KEY));
    let capabilities = Some(ModelCapabilities {
        accepted_attachment_kinds: Some(if images {
            vec![AttachmentKind::Image]
        } else {
            vec![]
        }),
        ..Default::default()
    });
    let rows: Vec<ModelRow> = names
        .iter()
        .map(|name| model_row(&server, name, capabilities.clone()))
        .collect();
    store_models(&s.ctx.db, &server, &rows);
    (server, rows)
}

pub(super) fn allow(s: &Setup, kind: &str, action: &str, target: &str, status: &str) {
    remember(
        &s.ctx.db,
        s.ctx.session.extension_id,
        (kind, action, target),
        status,
        1,
    );
}

pub(super) fn scripted(s: &Setup, scripts: &[(&str, Script)]) -> Arc<FakeSource> {
    let source = FakeSource::with(scripts);
    s.ctx.host.set_model_source(source.clone());
    source
}

pub(super) fn start(s: &Setup, params: &Value) -> Result<Value, BridgeError> {
    call(&s.ctx, "extension_model_job_start", params)
}

pub(super) fn prompt_of(outcome: Result<Value, BridgeError>) -> Value {
    let error = outcome.expect_err("the user is asked");
    assert_eq!(error.code.as_u16(), 1004, "{error:?}");
    error.details.unwrap()
}

pub(super) fn refused(outcome: Result<Value, BridgeError>) -> (u16, Value) {
    let error = outcome.expect_err("refused");
    (
        error.code.as_u16(),
        error.details.unwrap()["reason"].clone(),
    )
}

/// Waits for the completion event of the job without polling wall-clock time.
pub(super) fn completion(s: &Setup) -> Value {
    s.recorded
        .wait_for(|(name, payload)| name == FRAME_EVENT && payload["type"] == COMPLETED_EVENT)
        ["data"]
        .clone()
}

fn job_id(answer: &Value) -> Uuid {
    answer["jobId"].as_str().unwrap().parse().unwrap()
}

#[test]
fn a_granted_model_answers_and_the_extension_hears_the_result_with_its_provenance() {
    let s = setup();
    let (_, rows) = models(&s, "http://localhost:11434/v1", &["qwen3"], false);
    let source = scripted(&s, &[(&rows[0].id, Script::Answer(GOOD.into()))]);
    allow(&s, "model", "medium", &rows[0].id, "granted");

    let answer = start(&s, &job(vec![step()])).unwrap();

    assert_eq!(answer["status"], "running");
    let data = completion(&s);
    let result = json!({ "texts": ["Das ist ein Test."] });
    assert_eq!(
        data,
        json!({
            "jobId": job_id(&answer).to_string(),
            "status": "completed",
            "result": { "steps": [{ "id": "fix", "result": result }], "final": result },
            "provenance": [{
                "stepId": "fix", "formatName": "text-fix", "formatVersion": 1,
                "attempts": [{ "model": "qwen3", "place": "this_device", "effort": "none",
                               "outcome": "used" }]
            }]
        })
    );
    assert_eq!(source.asked(), [rows[0].id.clone()]);
    assert!(
        s.ctx.host.model_jobs().get(job_id(&answer)).is_none(),
        "holzi forgets the job after its event (FR-028)"
    );
}

#[test]
fn without_a_grant_the_user_is_asked_for_a_fitting_model_and_no_model_is_called() {
    let s = setup();
    let (_, rows) = models(&s, "http://localhost:11434/v1", &["a", "b"], false);
    let source = scripted(&s, &[]);
    let default = rows[1].id.clone();
    s.ctx
        .db
        .write_blocking(move |tx| {
            insert_or_update(tx, PrefScope::Vault, PREF_DEFAULT_MODEL, &default).map(drop)
        })
        .unwrap();
    let mut wish = step();
    wish["effort"] = json!("low");

    let details = prompt_of(start(&s, &job(vec![wish])));

    assert_eq!(
        details,
        json!({
            "resourceType": "model", "action": "low", "target": rows[1].id, "needs": ["text"]
        })
    );
    assert!(source.asked().is_empty());
    let asked = s.recorded.0.lock().unwrap().clone();
    assert!(asked.iter().any(|(name, _)| name == PERMISSION_REQUEST));
}

#[test]
fn no_fitting_model_is_no_capable_model_and_a_refused_one_is_consent_denied() {
    let s = setup();
    let (_, rows) = models(&s, "http://localhost:11434/v1", &["text"], false);
    scripted(&s, &[]);
    // Granted, but it takes no image.
    allow(&s, "model", "high", &rows[0].id, "granted");
    assert_eq!(
        refused(start(&s, &job(vec![image_step()]))),
        (8001, json!("no_capable_model"))
    );

    let (_, vision) = models(&s, "http://localhost:11434/v1", &["vision"], true);
    allow(&s, "model", "high", &vision[0].id, "denied");
    assert_eq!(
        refused(start(&s, &job(vec![image_step()]))),
        (1002, json!("consent_denied"))
    );
}

#[test]
fn a_model_outside_the_device_needs_consent_before_anything_is_sent() {
    let s = setup();
    let (cloud, rows) = models(&s, "https://api.example.com/v1", &["cloud"], true);
    let source = scripted(&s, &[(&rows[0].id, Script::Answer(GOOD.into()))]);
    allow(&s, "model", "low", &rows[0].id, "granted");

    let details = prompt_of(start(&s, &job(vec![image_step()])));

    assert_eq!(
        details,
        json!({
            "resourceType": "modelDestination", "action": "send",
            "target": cloud.id.to_string(),
            "transfer": { "kinds": ["text", "image"], "textBytes": 49, "imageCount": 1,
                          "imageBytes": 8, "earlierResult": false }
        })
    );
    assert!(source.asked().is_empty(), "nothing leaves without consent");

    allow(
        &s,
        "modelDestination",
        "send",
        &cloud.id.to_string(),
        "denied",
    );
    assert_eq!(
        refused(start(&s, &job(vec![image_step()]))),
        (1002, json!("consent_denied"))
    );
    assert!(source.asked().is_empty());

    // The user changes their mind: the same row now grants.
    allow(
        &s,
        "modelDestination",
        "send",
        &cloud.id.to_string(),
        "granted",
    );
    start(&s, &job(vec![image_step()])).unwrap();
    let data = completion(&s);
    assert_eq!(data["status"], "completed");
    assert_eq!(data["provenance"][0]["attempts"][0]["place"], "cloud");
    assert_eq!(source.asked(), [rows[0].id.clone()]);
}

#[test]
fn a_model_with_consent_runs_and_a_fallback_without_consent_is_never_tried() {
    let s = setup();
    let (_, local) = models(&s, "http://localhost:11434/v1", &["local"], false);
    let (cloud, remote) = models(&s, "https://api.example.com/v1", &["cloud"], false);
    let (_, other) = models(&s, "https://other.example.com/v1", &["other"], false);
    let source = scripted(
        &s,
        &[
            (&local[0].id, Script::Answer("".into())),
            (&remote[0].id, Script::Answer(GOOD.into())),
            (&other[0].id, Script::Answer(GOOD.into())),
        ],
    );
    allow(&s, "model", "none", "*", "granted");
    let ranked = vec![
        local[0].id.clone(),
        other[0].id.clone(),
        remote[0].id.clone(),
    ];
    let extension = s.ctx.session.extension_id;
    s.ctx
        .db
        .write_blocking(move |tx| {
            crate::extensions::permissions::order::set_order(tx, extension, &ranked)
                .map_err(Into::into)
        })
        .unwrap();
    allow(
        &s,
        "modelDestination",
        "send",
        &cloud.id.to_string(),
        "granted",
    );

    start(&s, &job(vec![step()])).unwrap();
    let data = completion(&s);

    // `other` has no consent: it is never tried, although it ranks before `cloud`.
    assert_eq!(source.asked(), [local[0].id.clone(), remote[0].id.clone()]);
    assert_eq!(data["status"], "completed");
    let attempts = &data["provenance"][0]["attempts"];
    assert_eq!(attempts[0]["outcome"], "unusable");
    assert_eq!(attempts[0]["reason"], "empty");
    assert_eq!(attempts[1]["place"], "cloud");
    assert_eq!(attempts[1]["outcome"], "used");
}

#[test]
fn a_refused_remote_destination_does_not_prompt_for_a_remote_fallback() {
    let s = setup();
    let (first, first_model) = models(&s, "https://first.example.com/v1", &["first"], false);
    let (_, second_model) = models(&s, "https://second.example.com/v1", &["second"], false);
    scripted(&s, &[]);
    allow(&s, "model", "none", "*", "granted");
    let extension = s.ctx.session.extension_id;
    let ranked = vec![first_model[0].id.clone(), second_model[0].id.clone()];
    s.ctx
        .db
        .write_blocking(move |tx| {
            crate::extensions::permissions::order::set_order(tx, extension, &ranked)
                .map_err(Into::into)
        })
        .unwrap();

    let first_prompt = prompt_of(start(&s, &job(vec![step()])));
    assert_eq!(first_prompt["target"], first.id.to_string());
    allow(
        &s,
        "modelDestination",
        "send",
        &first.id.to_string(),
        "denied",
    );

    assert_eq!(
        refused(start(&s, &job(vec![step()]))),
        (1002, json!("consent_denied"))
    );
    assert_eq!(
        s.recorded
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == PERMISSION_REQUEST)
            .count(),
        1,
        "the fallback must not trigger another consent prompt"
    );
}

#[test]
fn no_answer_or_event_holds_a_key_or_an_address() {
    let s = setup();
    let address = "http://10.0.0.5:8080/v1";
    let (server, rows) = models(&s, address, &["school"], false);
    scripted(
        &s,
        &[(
            &rows[0].id,
            Script::StreamFails(format!(
                "POST {address}/chat/completions with {KEY}: {PROVIDER_BODY}"
            )),
        )],
    );
    allow(&s, "model", "low", &rows[0].id, "granted");
    allow(
        &s,
        "modelDestination",
        "send",
        &server.id.to_string(),
        "granted",
    );

    let answer = start(&s, &job(vec![step()])).unwrap();
    let data = completion(&s);
    let list = call(&s.ctx, "extension_model_list", &json!({})).unwrap();

    assert_eq!(data["status"], "failed");
    assert_eq!(data["error"]["code"], "unavailable");
    assert_eq!(data["error"]["stepId"], "fix");
    for text in [answer.to_string(), data.to_string(), list.to_string()] {
        for secret in [KEY, address, "10.0.0.5", PROVIDER_BODY] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
    }
}

#[test]
fn a_successful_result_never_carries_a_key_or_an_address() {
    let s = setup();
    let address = "http://10.0.0.5:8080/v1";
    let (server, rows) = models(&s, address, &["school"], false);
    scripted(
        &s,
        &[(
            &rows[0].id,
            Script::Answer(format!(r#"{{"texts":["{KEY} {address}"]}}"#)),
        )],
    );
    allow(&s, "model", "low", &rows[0].id, "granted");
    allow(
        &s,
        "modelDestination",
        "send",
        &server.id.to_string(),
        "granted",
    );

    let answer = start(&s, &job(vec![step()])).unwrap();
    let data = completion(&s);

    assert_eq!(data["status"], "failed");
    assert_eq!(data["error"]["code"], "unusable");
    for secret in [KEY, address, "10.0.0.5"] {
        assert!(!answer.to_string().contains(secret));
        assert!(!data.to_string().contains(secret));
    }
}

#[test]
fn the_list_names_the_granted_models_with_place_capabilities_and_ceiling() {
    let s = setup();
    let models_of = |s: &Setup| call(&s.ctx, "extension_model_list", &json!({})).unwrap();
    let (_, vision) = models(&s, "http://localhost:11434/v1", &["vision"], true);
    let (_, cloud) = models(&s, "https://api.example.com/v1", &["cloud"], false);
    assert_eq!(models_of(&s), json!({ "models": [] }), "no grant, no model");

    allow(&s, "model", "high", &vision[0].id, "granted");
    allow(&s, "model", "low", &cloud[0].id, "granted");

    let listed = models_of(&s);
    let entries = listed["models"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    let vision_entry = entries.iter().find(|m| m["name"] == "vision").unwrap();
    assert_eq!(
        *vision_entry,
        json!({
            "id": vision[0].id, "name": "vision", "provider": "Test provider",
            "place": "this_device", "capabilities": { "image": true, "effort": "none" },
            "maxEffort": "high"
        })
    );
    let cloud_entry = entries.iter().find(|m| m["name"] == "cloud").unwrap();
    assert_eq!(cloud_entry["place"], "cloud");
    assert_eq!(cloud_entry["capabilities"]["image"], false);
    assert_eq!(cloud_entry["maxEffort"], "low");
}

#[test]
fn a_frame_runs_two_jobs_at_most_and_a_closed_frame_hears_nothing() {
    let s = setup();
    let (_, rows) = models(&s, "http://localhost:11434/v1", &["slow"], false);
    let source = scripted(&s, &[(&rows[0].id, Script::Hang)]);
    allow(&s, "model", "low", &rows[0].id, "granted");

    start(&s, &job(vec![step()])).unwrap();
    start(&s, &job(vec![step()])).unwrap();
    assert_eq!(
        refused(start(&s, &job(vec![step()]))),
        (7000, json!("active_jobs"))
    );

    let owner = JobOwner {
        extension_id: s.ctx.session.extension_id,
        frame: s.ctx.session.frame.clone(),
    };
    source.wait_until_asked(2);
    assert_eq!(s.ctx.host.model_jobs().cancel_owned_by(&owner), 2);
    source.wait_until_dropped(2);
    assert_eq!(source.dropped.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert!(
        !s.recorded
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|(_, payload)| payload["type"] == COMPLETED_EVENT),
        "no event to a closed frame"
    );
}

#[test]
fn a_wish_above_the_ceiling_is_capped_and_the_provenance_names_what_was_sent() {
    let s = setup();
    let server = provider(
        ProviderKind::ApiKey,
        Some("http://localhost:11434/v1"),
        None,
    );
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
        ..Default::default()
    };
    let row = model_row(&server, "thinker", Some(levels));
    store_models(&s.ctx.db, &server, std::slice::from_ref(&row));
    let source = scripted(&s, &[(&row.id, Script::Answer(GOOD.into()))]);
    allow(&s, "model", "medium", &row.id, "granted");
    let mut wish = step();
    wish["effort"] = json!("high");

    start(&s, &job(vec![wish])).unwrap();
    let data = completion(&s);

    assert_eq!(data["provenance"][0]["attempts"][0]["effort"], "medium");
    let seen = source.seen.lock().unwrap();
    assert_eq!(seen[0].1.reasoning_option.as_deref(), Some("medium"));
}

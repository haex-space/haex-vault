//! The way from an extension to its grant (spec 055 US9, T088/T090): asking for access names the
//! granted models that fit or asks like the first job, without sending anything and without asking
//! for a destination; opening the model settings tells holzi's window about the calling extension
//! only.

use serde_json::{json, Value};

use super::dispatch::call;
use super::dispatch::tests::{setup, Setup};
use super::model_jobs_run_tests::{allow, models, prompt_of, refused, scripted, GOOD};
use crate::extensions::bridge::dispatch::METHODS;
use crate::extensions::error::BridgeError;
use crate::model_jobs::access::MODEL_SETTINGS_EVENT;
use crate::model_jobs::test_support::Script;

fn request(s: &Setup, params: Value) -> Result<Value, BridgeError> {
    call(&s.ctx, "extension_model_access_request", &params)
}

#[test]
fn both_methods_are_on_the_allowlist() {
    for name in [
        "extension_model_access_request",
        "extension_model_settings_open",
    ] {
        let method = METHODS.iter().find(|m| m.name == name).expect(name);
        assert!(method.module.ends_with("model_jobs"), "{name}");
    }
}

#[test]
fn a_granted_model_that_fits_is_named_without_a_question() {
    let s = setup();
    let (_, text) = models(&s, "http://localhost:11434/v1", &["text"], false);
    let (_, vision) = models(&s, "http://localhost:11434/v1", &["vision"], true);
    let source = scripted(&s, &[(&vision[0].id, Script::Answer(GOOD.into()))]);
    allow(&s, "model", "medium", &text[0].id, "granted");
    allow(&s, "model", "high", &vision[0].id, "granted");

    let answer = request(&s, json!({ "needs": ["text", "image"] })).expect("no question");
    let listed = answer["models"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "only the model that takes images");
    assert_eq!(listed[0]["id"], json!(vision[0].id));
    assert_eq!(listed[0]["maxEffort"], "high");
    assert!(source.asked().is_empty(), "nothing is sent");

    let text_only = request(&s, json!({ "needs": ["text"] })).unwrap();
    assert_eq!(text_only["models"].as_array().unwrap().len(), 2);
}

#[test]
fn without_a_fitting_grant_holzi_asks_like_for_the_first_job() {
    let s = setup();
    let (_, text) = models(&s, "http://localhost:11434/v1", &["text"], false);
    let (_, vision) = models(&s, "https://api.example.com/v1", &["vision"], true);
    let source = scripted(&s, &[(&vision[0].id, Script::Answer(GOOD.into()))]);
    allow(&s, "model", "medium", &text[0].id, "granted");

    let details = prompt_of(request(&s, json!({ "needs": ["text", "image"] })));
    assert_eq!(details["resourceType"], "model");
    // The user chooses the effort with the model; the question names none.
    assert_eq!(details["action"], "none");
    assert_eq!(details["target"], json!(vision[0].id));
    assert_eq!(details["needs"], json!(["text", "image"]));

    // Granted, the model is named; its destination outside this device is not asked about.
    allow(&s, "model", "low", &vision[0].id, "granted");
    let answer = request(&s, json!({ "needs": ["text", "image"] })).expect("no question");
    assert_eq!(answer["models"][0]["id"], json!(vision[0].id));
    assert_eq!(answer["models"][0]["place"], "cloud");
    assert!(source.asked().is_empty(), "nothing is sent");
}

#[test]
fn no_model_that_fits_and_a_refusal_answer_like_a_job() {
    let s = setup();
    let (_, text) = models(&s, "http://localhost:11434/v1", &["text"], false);
    assert_eq!(
        refused(request(&s, json!({ "needs": ["text", "image"] }))),
        (8001, json!("no_capable_model"))
    );

    let (_, vision) = models(&s, "http://localhost:11434/v1", &["vision"], true);
    allow(&s, "model", "high", &vision[0].id, "denied");
    allow(&s, "model", "medium", &text[0].id, "granted");
    assert_eq!(
        refused(request(&s, json!({ "needs": ["text", "image"] }))),
        (1002, json!("consent_denied"))
    );
}

#[test]
fn a_wrong_form_answers_3001() {
    let s = setup();
    for params in [
        json!({}),
        json!({ "needs": ["sound"] }),
        json!({ "needs": ["text", "text"] }),
        // The extension has no say about the effort.
        json!({ "needs": ["text"], "effort": "low" }),
        json!({ "needs": ["text"], "model": "x" }),
    ] {
        let error = request(&s, params.clone()).expect_err("refused");
        assert_eq!(error.code.as_u16(), 3001, "{params}");
    }
}

#[test]
fn opening_the_model_settings_names_the_calling_extension_only() {
    let s = setup();
    let answer = call(
        &s.ctx,
        "extension_model_settings_open",
        &json!({ "extensionId": "00000000-0000-0000-0000-000000000000" }),
    )
    .expect("opened");
    assert_eq!(answer, json!({}));
    let payload = s
        .recorded
        .wait_for(|(name, _)| name == MODEL_SETTINGS_EVENT);
    assert_eq!(
        payload,
        json!({ "extensionId": s.ctx.session.extension_id.to_string() })
    );
}

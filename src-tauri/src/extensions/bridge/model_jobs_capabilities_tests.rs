//! What the user states about a model of an OpenAI-compatible server reaches the readers (spec 055
//! T054, FR-019): before the statement a model the server describes with nothing fits no image
//! step; after it, `extension_model_list` names it as taking images and a job with an image runs on
//! it. A model without images never gets an image job (T064, SC-006). The list names the granted
//! models with place, capabilities and ceiling (T029).

use serde_json::json;

use super::dispatch::call;
use super::dispatch::tests::{setup, Setup};
use super::model_jobs_run_tests::{
    allow, completion, image_step, job, models, prompt_of, refused, scripted, start, GOOD,
};
use crate::model_jobs::test_support::{model_row, provider, store_models, Script};
use crate::providers::capabilities::{set_user_capabilities, SetUserCapabilitiesArgs};
use crate::storage::providers::ProviderKind;

#[test]
fn a_statement_about_images_reaches_the_list_and_the_choice_of_models() {
    let s = setup();
    let server = provider(
        ProviderKind::ApiKey,
        Some("http://localhost:11434/v1"),
        None,
    );
    // The server's model list says nothing about what a model can do.
    let row = model_row(&server, "qwen3-vl", None);
    store_models(&s.ctx.db, &server, std::slice::from_ref(&row));
    let source = scripted(&s, &[(&row.id, Script::Answer(GOOD.into()))]);
    allow(&s, "model", "medium", &row.id, "granted");

    assert_eq!(
        refused(start(&s, &job(vec![image_step()]))),
        (8001, json!("no_capable_model")),
        "not known to take images"
    );

    tauri::async_runtime::block_on(set_user_capabilities(
        &s.ctx.db,
        SetUserCapabilitiesArgs {
            model_id: row.id.clone(),
            accepts_images: Some(true),
            effort_levels: Some(vec!["low".into(), "high".into()]),
        },
    ))
    .expect("stated");

    let listed = call(&s.ctx, "extension_model_list", &json!({})).unwrap();
    assert_eq!(listed["models"][0]["id"], json!(row.id));
    assert_eq!(listed["models"][0]["capabilities"]["image"], true);
    start(&s, &job(vec![image_step()])).expect("the job starts");
    let data = completion(&s);
    assert_eq!(data["status"], "completed");
    assert_eq!(source.asked(), std::slice::from_ref(&row.id));
}

#[test]
fn an_image_job_never_reaches_a_model_without_images() {
    let s = setup();
    let (_, text) = models(&s, "http://localhost:11434/v1", &["text"], false);
    let (_, vision) = models(&s, "http://localhost:11434/v1", &["vision"], true);
    let source = scripted(
        &s,
        &[
            (&text[0].id, Script::Answer(GOOD.into())),
            (&vision[0].id, Script::Answer(GOOD.into())),
        ],
    );
    allow(&s, "model", "medium", &text[0].id, "granted");

    // Only the text model is granted: holzi asks for the one that takes images.
    let details = prompt_of(start(&s, &job(vec![image_step()])));
    assert_eq!(details["target"], json!(vision[0].id));
    assert_eq!(details["needs"], json!(["text", "image"]));
    assert!(source.asked().is_empty(), "nothing is sent before");

    // Granted, the vision model reads the image although the text model ranks first.
    allow(&s, "model", "medium", &vision[0].id, "granted");
    start(&s, &job(vec![image_step()])).expect("the job starts");
    assert_eq!(completion(&s)["status"], "completed");
    assert_eq!(source.asked(), std::slice::from_ref(&vision[0].id));
}

#[test]
fn without_any_model_that_takes_images_an_image_job_is_refused_before_anything_is_sent() {
    let s = setup();
    let (_, text) = models(&s, "http://localhost:11434/v1", &["text"], false);
    let source = scripted(&s, &[(&text[0].id, Script::Answer(GOOD.into()))]);
    allow(&s, "model", "medium", &text[0].id, "granted");

    assert_eq!(
        refused(start(&s, &job(vec![image_step()]))),
        (8001, json!("no_capable_model"))
    );
    assert!(source.asked().is_empty());
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

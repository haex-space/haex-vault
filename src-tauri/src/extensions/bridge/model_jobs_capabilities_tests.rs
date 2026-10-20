//! What the user states about a model of an OpenAI-compatible server reaches the readers (spec 055
//! T054, FR-019): before the statement a model the server describes with nothing fits no image
//! step; after it, `extension_model_list` names it as taking images and a job with an image runs on
//! it.

use serde_json::json;

use super::dispatch::call;
use super::dispatch::tests::setup;
use super::model_jobs_run_tests::{
    allow, completion, image_step, job, refused, scripted, start, GOOD,
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

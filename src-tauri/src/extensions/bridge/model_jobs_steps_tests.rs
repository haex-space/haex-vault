//! A job of several steps through the bridge (spec 055 T041, FR-025): every step is checked before
//! the first one runs, and the completion event carries every step's result and provenance.

use serde_json::json;

use super::dispatch::tests::setup;
use super::model_jobs_run_tests::{
    allow, completion, image_step, job, models, prompt_of, refused, scripted, start, step, GOOD,
};
use crate::model_jobs::test_support::Script;

fn fix_from_read() -> serde_json::Value {
    let mut fix = step();
    fix["input"] = json!({ "from": { "step": "read", "pointer": "/texts" } });
    fix["format"] = json!({ "name": "text-fix", "version": 2, "schema": {
        "type": "object", "required": ["fixed"],
        "properties": { "fixed": { "type": "string" } }
    }});
    fix
}

fn read() -> serde_json::Value {
    let mut read = step();
    read["id"] = json!("read");
    read
}

#[test]
fn two_steps_run_in_order_and_the_event_carries_both_results() {
    let s = setup();
    let (_, rows) = models(
        &s,
        "http://localhost:11434/v1",
        &["a-reader", "b-fixer"],
        false,
    );
    let source = scripted(
        &s,
        &[
            (&rows[0].id, Script::Answer(r#"{"texts":["Tsest"]}"#.into())),
            (&rows[1].id, Script::Answer(r#"{"fixed":"Test"}"#.into())),
        ],
    );
    allow(&s, "model", "low", "*", "granted");

    start(&s, &job(vec![read(), fix_from_read()])).unwrap();
    let data = completion(&s);

    assert_eq!(data["status"], "completed");
    assert_eq!(
        data["result"],
        json!({
            "steps": [
                { "id": "read", "result": { "texts": ["Tsest"] } },
                { "id": "fix", "result": { "fixed": "Test" } }
            ],
            "final": { "fixed": "Test" }
        })
    );
    let provenance = data["provenance"].as_array().unwrap();
    assert_eq!(provenance.len(), 2);
    assert_eq!(provenance[0]["stepId"], "read");
    assert_eq!(provenance[0]["attempts"].as_array().unwrap().len(), 1);
    // Each step tries the rank from the top: the reader's answer misses the second format.
    assert_eq!(provenance[1]["stepId"], "fix");
    assert_eq!(provenance[1]["formatVersion"], 2);
    assert_eq!(provenance[1]["attempts"][0]["model"], "a-reader");
    assert_eq!(provenance[1]["attempts"][0]["outcome"], "unusable");
    assert_eq!(provenance[1]["attempts"][1]["model"], "b-fixer");
    assert_eq!(provenance[1]["attempts"][1]["outcome"], "used");
    assert_eq!(
        source.asked(),
        [rows[0].id.clone(), rows[0].id.clone(), rows[1].id.clone()]
    );
}

#[test]
fn a_later_step_without_a_model_refuses_the_start_before_anything_runs() {
    let s = setup();
    let (_, rows) = models(&s, "http://localhost:11434/v1", &["text"], false);
    let source = scripted(&s, &[(&rows[0].id, Script::Answer(GOOD.into()))]);
    allow(&s, "model", "low", &rows[0].id, "granted");
    let mut photo = image_step();
    photo["id"] = json!("photo");

    // No model of the vault takes images.
    assert_eq!(
        refused(start(&s, &job(vec![read(), photo.clone()]))),
        (8001, json!("no_capable_model"))
    );

    // A model that takes images exists but is not granted: the user is asked first.
    let (_, vision) = models(&s, "http://localhost:11434/v1", &["vision"], true);
    let details = prompt_of(start(&s, &job(vec![read(), photo])));
    assert_eq!(details["resourceType"], "model");
    assert_eq!(details["target"], vision[0].id);

    assert!(source.asked().is_empty(), "the first step never ran");
}

//! holzi never picks another model on its own (spec 055 FR-007, FR-027, change of 2026-11-14): the
//! default model gets the step and nobody else; when it cannot take the step or is not available the
//! user is asked before any request is sent, and when it fails while the job runs the user is asked
//! at once. Without a default the first model the extension may use goes first and a failure asks too.

use serde_json::json;

use super::dispatch::tests::{setup, Setup};
use super::model_jobs_run_tests::{
    allow, answer_replacement, completion, image_step, job, models, refused, scripted, start, step,
    GOOD,
};
use crate::model_jobs::test_support::Script;
use crate::storage::models::ModelRow;

fn set_default(s: &Setup, model_id: &str) {
    let (extension, model_id) = (s.ctx.session.extension_id, model_id.to_owned());
    s.ctx
        .db
        .write_blocking(move |tx| {
            crate::extensions::permissions::default_model::set(tx, extension, &model_id)
                .map_err(Into::into)
        })
        .unwrap();
}

fn default_of(s: &Setup) -> Option<String> {
    let extension = s.ctx.session.extension_id;
    s.ctx
        .db
        .read_blocking(move |q| {
            crate::extensions::permissions::default_model::get(q, extension).map_err(Into::into)
        })
        .unwrap()
}

/// A text-only model and one that reads images, on two servers of this device.
fn text_and_vision(s: &Setup) -> (ModelRow, ModelRow) {
    let (_, text) = models(s, "http://localhost:11434/v1", &["text-only"], false);
    let (_, vision) = models(s, "http://localhost:11435/v1", &["vision"], true);
    (text[0].clone(), vision[0].clone())
}

fn names(question: &serde_json::Value) -> Vec<String> {
    question["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_default_that_cannot_read_images_asks_before_any_request_and_the_choice_can_be_the_new_default()
{
    let s = setup();
    let (text, vision) = text_and_vision(&s);
    let source = scripted(
        &s,
        &[
            (&text.id, Script::Answer(GOOD.into())),
            (&vision.id, Script::Answer(GOOD.into())),
        ],
    );
    allow(&s, "model", "none", "*", "granted");
    set_default(&s, &text.id);

    start(&s, &job(vec![image_step()])).unwrap();
    let question = answer_replacement(&s, Some((&vision.id, "none", true)));
    let data = completion(&s);

    assert_eq!(question["failedModel"], "text-only");
    assert_eq!(question["failedWasDefault"], true);
    assert_eq!(question["outcome"], "cannot_take");
    assert_eq!(question["reason"], "it cannot take images");
    assert_eq!(names(&question), ["vision"]);
    assert_eq!(data["status"], "completed");
    assert_eq!(
        source.asked(),
        std::slice::from_ref(&vision.id),
        "the default was never asked, only the model the user chose"
    );
    let attempts = &data["provenance"][0]["attempts"];
    assert_eq!(attempts[0]["model"], "text-only");
    assert_eq!(attempts[0]["outcome"], "unavailable");
    assert_eq!(attempts[0]["effort"], "none");
    assert_eq!(attempts[0]["reason"], "it cannot take images");
    assert_eq!(attempts[1]["model"], "vision");
    assert_eq!(attempts[1]["outcome"], "used");
    assert_eq!(default_of(&s), Some(vision.id.clone()));
}

#[test]
fn declining_the_question_before_the_start_ends_the_step_and_nothing_is_sent() {
    let s = setup();
    let (text, vision) = text_and_vision(&s);
    let source = scripted(
        &s,
        &[
            (&text.id, Script::Answer(GOOD.into())),
            (&vision.id, Script::Answer(GOOD.into())),
        ],
    );
    allow(&s, "model", "none", "*", "granted");
    set_default(&s, &text.id);

    start(&s, &job(vec![image_step()])).unwrap();
    answer_replacement(&s, None);
    let data = completion(&s);

    assert_eq!(data["status"], "failed");
    assert_eq!(data["error"]["code"], "no_capable_model");
    assert_eq!(data["error"]["stepId"], "fix");
    assert!(
        source.asked().is_empty(),
        "no model was asked behind the user's back"
    );
    assert_eq!(default_of(&s), Some(text.id.clone()), "nothing changed");
}

#[test]
fn a_default_the_extension_may_not_use_any_more_asks_and_the_granted_model_is_offered_first() {
    let s = setup();
    let (_, rows) = models(
        &s,
        "http://localhost:11434/v1",
        &["old-default", "granted"],
        false,
    );
    let source = scripted(
        &s,
        &[
            (&rows[0].id, Script::Answer(GOOD.into())),
            (&rows[1].id, Script::Answer(GOOD.into())),
        ],
    );
    // Only the second one is allowed; the default row still names the first.
    allow(&s, "model", "none", &rows[1].id, "granted");
    set_default(&s, &rows[0].id);

    start(&s, &job(vec![step()])).unwrap();
    let question = answer_replacement(&s, Some((&rows[1].id, "none", false)));
    let data = completion(&s);

    assert_eq!(question["failedModel"], "old-default");
    assert_eq!(question["outcome"], "not_available");
    assert_eq!(names(&question), ["granted"]);
    assert_eq!(data["status"], "completed");
    assert_eq!(source.asked(), std::slice::from_ref(&rows[1].id));
    assert_eq!(
        data["provenance"][0]["attempts"][0]["outcome"],
        "unavailable"
    );
}

#[test]
fn a_default_the_vault_does_not_have_any_more_asks_without_naming_it() {
    let s = setup();
    let (_, rows) = models(&s, "http://localhost:11434/v1", &["fine"], false);
    let source = scripted(&s, &[(&rows[0].id, Script::Answer(GOOD.into()))]);
    allow(&s, "model", "none", &rows[0].id, "granted");
    set_default(&s, "ghost-model");

    start(&s, &job(vec![step()])).unwrap();
    let question = answer_replacement(&s, Some((&rows[0].id, "none", false)));
    let data = completion(&s);

    assert_eq!(question["failedModel"], "");
    assert_eq!(question["outcome"], "not_available");
    assert_eq!(data["status"], "completed");
    assert_eq!(
        data["provenance"][0]["attempts"].as_array().unwrap().len(),
        1,
        "a model the vault does not know has no name to list"
    );
    assert_eq!(source.asked(), std::slice::from_ref(&rows[0].id));
}

#[test]
fn a_default_that_cannot_take_the_step_with_no_other_model_able_to_is_no_capable_model() {
    let s = setup();
    let (_, rows) = models(
        &s,
        "http://localhost:11434/v1",
        &["text-only", "text-too"],
        false,
    );
    scripted(&s, &[]);
    allow(&s, "model", "none", "*", "granted");
    set_default(&s, &rows[0].id);

    assert_eq!(
        refused(start(&s, &job(vec![image_step()]))),
        (8001, json!("no_capable_model"))
    );
}

#[test]
fn a_failing_default_asks_at_once_and_the_other_granted_models_are_not_tried() {
    let s = setup();
    let (_, rows) = models(
        &s,
        "http://localhost:11434/v1",
        &["broken", "ready", "spare"],
        false,
    );
    let source = scripted(
        &s,
        &[
            (&rows[0].id, Script::Answer("".into())),
            (&rows[1].id, Script::Answer(GOOD.into())),
            (&rows[2].id, Script::Answer(GOOD.into())),
        ],
    );
    allow(&s, "model", "none", "*", "granted");
    set_default(&s, &rows[0].id);

    start(&s, &job(vec![step()])).unwrap();
    let question = answer_replacement(&s, None);
    let data = completion(&s);

    assert_eq!(question["outcome"], "unusable");
    assert_eq!(question["failedWasDefault"], true);
    assert_eq!(data["status"], "failed");
    assert_eq!(data["error"]["code"], "unusable");
    assert_eq!(
        source.asked(),
        std::slice::from_ref(&rows[0].id),
        "only the default was asked"
    );
    assert_eq!(
        data["provenance"][0]["attempts"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn without_a_default_the_first_granted_model_goes_first_and_its_failure_asks() {
    let s = setup();
    let (_, rows) = models(&s, "http://localhost:11434/v1", &["first", "second"], false);
    let source = scripted(
        &s,
        &[
            (&rows[0].id, Script::Answer("".into())),
            (&rows[1].id, Script::Answer(GOOD.into())),
        ],
    );
    allow(&s, "model", "none", "*", "granted");

    start(&s, &job(vec![step()])).unwrap();
    let question = answer_replacement(&s, Some((&rows[1].id, "none", false)));
    let data = completion(&s);

    assert_eq!(question["failedModel"], "first");
    assert_eq!(question["failedWasDefault"], false);
    assert_eq!(data["status"], "completed");
    assert_eq!(source.asked(), [rows[0].id.clone(), rows[1].id.clone()]);
    assert_eq!(default_of(&s), None);
}

#[test]
fn without_a_default_a_declined_question_never_tries_the_second_granted_model() {
    let s = setup();
    let (_, rows) = models(&s, "http://localhost:11434/v1", &["first", "second"], false);
    let source = scripted(
        &s,
        &[
            (&rows[0].id, Script::Answer("".into())),
            (&rows[1].id, Script::Answer(GOOD.into())),
        ],
    );
    allow(&s, "model", "none", "*", "granted");

    start(&s, &job(vec![step()])).unwrap();
    answer_replacement(&s, None);
    let data = completion(&s);

    assert_eq!(data["status"], "failed");
    assert_eq!(data["error"]["code"], "unusable");
    assert_eq!(source.asked(), std::slice::from_ref(&rows[0].id));
}

#[test]
fn a_default_that_fits_the_first_step_but_not_the_second_asks_for_the_second_only() {
    let s = setup();
    let (text, vision) = text_and_vision(&s);
    let source = scripted(
        &s,
        &[
            (&text.id, Script::Answer(GOOD.into())),
            (&vision.id, Script::Answer(GOOD.into())),
        ],
    );
    allow(&s, "model", "none", "*", "granted");
    set_default(&s, &text.id);
    let mut look = image_step();
    look["id"] = json!("look");

    start(&s, &job(vec![step(), look])).unwrap();
    let question = answer_replacement(&s, Some((&vision.id, "none", false)));
    let data = completion(&s);

    assert_eq!(question["outcome"], "cannot_take");
    assert_eq!(data["status"], "completed");
    assert_eq!(source.asked(), [text.id.clone(), vision.id.clone()]);
    let steps = data["provenance"].as_array().unwrap();
    assert_eq!(
        steps[0]["attempts"].as_array().unwrap().len(),
        1,
        "step one asked nobody"
    );
    assert_eq!(steps[1]["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(
        default_of(&s),
        Some(text.id.clone()),
        "no tick, the default stays"
    );
}

//! The page recognition of haex-notes as the first job (spec 055 US7, T066), with the format the
//! test extension describes (`tests/fixtures/extension_e2e/model-jobs/page-format.json`): holzi
//! checks it like any other format. A table keeps its empty cells, a missing cell drops the answer,
//! and a sideways photo comes back with the model's own hint, not turned by holzi.

use serde_json::{json, Value};

use super::dispatch::tests::setup;
use super::model_jobs_run_tests::{allow, completion, job, models, scripted, start};
use crate::model_jobs::test_support::Script;

const PAGE_FORMAT: &str =
    include_str!("../../../tests/fixtures/extension_e2e/model-jobs/page-format.json");

fn read_step() -> Value {
    json!({
        "id": "read",
        "needs": ["text", "image"],
        "instruction": "Zerlege die Seite in Regionen. Gib nur JSON aus.",
        "input": { "images": [{ "mimeType": "image/png", "data": "iVBORw0KGgo=" }] },
        "format": serde_json::from_str::<Value>(PAGE_FORMAT).expect("page format"),
    })
}

/// Runs the read step on a local vision model that answers `answer`, and returns the end.
fn read(answer: &Value) -> Value {
    let s = setup();
    let (_, rows) = models(&s, "http://localhost:11434/v1", &["vision"], true);
    scripted(&s, &[(&rows[0].id, Script::Answer(answer.to_string()))]);
    allow(&s, "model", "medium", &rows[0].id, "granted");
    start(&s, &job(vec![read_step()])).expect("started");
    completion(&s)
}

#[test]
fn a_page_with_text_image_and_table_passes_with_its_empty_cells() {
    let page = json!({ "regions": [
        { "kind": "text", "box": [85, 228, 623, 323], "text": "5. Reaktion von Säurelösungen\nmit Metallen" },
        { "kind": "image", "box": [102, 340, 412, 453] },
        { "kind": "table", "box": [58, 470, 978, 986],
          "rows": [["Tag", "Fach"], ["Mo", "Mathematik"], ["Di", ""]] }
    ]});

    let end = read(&page);

    assert_eq!(end["status"], "completed", "{end}");
    assert_eq!(end["result"]["final"], page);
}

#[test]
fn a_formula_line_and_a_formula_cell_pass_with_their_latex() {
    let page = json!({ "regions": [
        { "kind": "formula", "box": [60, 300, 700, 360],
          "latex": "\\ce{H2SO3 + H2O -> H3O+ + HSO3-}" },
        { "kind": "table", "box": [58, 470, 978, 986],
          "rows": [["Salz", "Formel"], ["Natriumcarbonat", "$\\ce{Na2CO3}$"]] }
    ]});

    let end = read(&page);

    assert_eq!(end["status"], "completed", "{end}");
    assert_eq!(end["result"]["final"], page);
}

#[test]
fn a_region_of_an_unknown_kind_is_unusable() {
    let page = json!({ "regions": [
        { "kind": "equation", "box": [60, 300, 700, 360], "latex": "a^2 + b^2 = c^2" }
    ]});

    let end = read(&page);

    assert_eq!(end["status"], "failed", "{end}");
    assert_eq!(end["error"]["code"], "unusable");
}

#[test]
fn a_table_with_a_missing_cell_is_unusable() {
    let page = json!({ "regions": [
        { "kind": "table", "box": [58, 67, 978, 986], "rows": [["Tag", "Fach"], ["Mo", null]] }
    ]});

    let end = read(&page);

    assert_eq!(end["status"], "failed", "{end}");
    assert_eq!(end["error"]["code"], "unusable");
    assert!(
        end.get("result").is_none(),
        "nothing of the page is delivered"
    );
    assert_eq!(end["provenance"][0]["attempts"][0]["outcome"], "unusable");
}

#[test]
fn a_sideways_photo_returns_the_models_hint_without_turning_anything() {
    let page = json!({
        "regions": [{ "kind": "text", "box": [40, 100, 960, 300], "text": "Zur Auswahl" }],
        "rotation": 90
    });

    let end = read(&page);

    assert_eq!(end["status"], "completed", "{end}");
    assert_eq!(
        end["result"]["final"], page,
        "the hint and the boxes are the model's"
    );
}

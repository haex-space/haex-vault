//! `codex_models.rs`: what a `model/list` answer becomes as model rows, and the run of the
//! program that asks for it. The answer's shape is the one `codex app-server
//! generate-json-schema` describes for `ModelListResponse` (codex 0.169.0).

use serde_json::json;

use super::{LEGACY_MODEL_ID, list_models, parse_models};
use crate::adapters::types::AttachmentKind;
use crate::model_capabilities::ReasoningControl;

fn answer() -> serde_json::Value {
    json!({
        "data": [
            {
                "id": "gpt-5.5", "model": "gpt-5.5", "displayName": "GPT-5.5",
                "hidden": false, "isDefault": true,
                "defaultReasoningEffort": "medium",
                "supportedReasoningEfforts": [
                    {"reasoningEffort": "low", "description": "Fast"},
                    {"reasoningEffort": "medium", "description": "Balanced"},
                    {"reasoningEffort": "high", "description": "Deep"}
                ],
                "inputModalities": ["text", "image"]
            },
            {
                "id": "gpt-5.5-mini", "model": "gpt-5.5-mini", "displayName": "GPT-5.5 Mini",
                "hidden": false, "isDefault": false,
                "supportedReasoningEfforts": [],
                "inputModalities": ["text"]
            },
            {
                "id": "internal", "model": "internal", "displayName": "Internal",
                "hidden": true, "isDefault": false
            }
        ],
        "nextCursor": null
    })
}

#[test]
fn the_models_of_an_answer_become_rows_without_the_hidden_ones() {
    let models = parse_models(&answer());
    let ids: Vec<_> = models.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(ids, ["gpt-5.5", "gpt-5.5-mini"]);
    assert_eq!(models[0].display_name, "GPT-5.5");
}

#[test]
fn the_reasoning_efforts_and_the_image_input_of_a_model_are_kept() {
    let models = parse_models(&answer());

    let Some(ReasoningControl::Presets { options }) = &models[0].capabilities.reasoning else {
        panic!("a model with efforts offers presets");
    };
    let efforts: Vec<_> = options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(efforts, ["low", "medium", "high"]);
    assert_eq!(
        models[0].capabilities.accepted_attachment_kinds,
        Some(vec![AttachmentKind::Text, AttachmentKind::Image])
    );

    // No efforts listed says nothing about reasoning; text only is a statement about images.
    assert_eq!(models[1].capabilities.reasoning, None);
    assert_eq!(
        models[1].capabilities.accepted_attachment_kinds,
        Some(vec![AttachmentKind::Text])
    );
}

#[test]
fn an_answer_without_a_list_or_a_model_name_gives_no_row() {
    assert!(parse_models(&json!({})).is_empty());
    assert!(parse_models(&json!({"data": [{"displayName": "No id"}]})).is_empty());
    // The name falls back to the id, the id to the `id` field.
    let models = parse_models(&json!({"data": [{"id": "only-id"}]}));
    assert_eq!(models[0].remote_id, "only-id");
    assert_eq!(models[0].display_name, "only-id");
}

#[test]
fn the_placeholder_of_the_time_before_the_list_names_no_model() {
    use super::super::codex::chosen_model;
    assert_eq!(chosen_model(LEGACY_MODEL_ID), None);
    assert_eq!(chosen_model(""), None);
    assert_eq!(chosen_model("  "), None);
    assert_eq!(chosen_model("gpt-5.5"), Some("gpt-5.5"));
}

#[tokio::test]
async fn without_credentials_nothing_is_started() {
    let dir = tempfile::TempDir::new().unwrap();
    let runtime = crate::cli_tools::runtime::DelegateRuntime::for_stub(
        "codex",
        dir.path().join("does-not-exist"),
        dir.path(),
    );
    let error = list_models(runtime, Vec::new()).await.unwrap_err();
    assert!(matches!(
        error,
        crate::adapters::AdapterError::InvalidCredentials
    ));
}

/// A stand-in `codex app-server`: answers `initialize`, then `model/list` in two pages, as the
/// real program does over its JSON lines.
#[cfg(unix)]
fn stub_program(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("stub-codex");
    std::fs::write(
        &path,
        r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      echo '{"id":1,"result":{"userAgent":"stub"}}' ;;
    *'"method":"model/list"'*'"cursor":null'*)
      echo '{"id":2,"result":{"data":[{"id":"a","model":"a","displayName":"A"}],"nextCursor":"page2"}}' ;;
    *'"method":"model/list"'*)
      echo '{"id":3,"result":{"data":[{"id":"b","model":"b","displayName":"B"}],"nextCursor":null}}'
      exit 0 ;;
  esac
done
"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[tokio::test]
async fn the_program_is_asked_page_by_page_and_the_pages_are_joined() {
    let dir = tempfile::TempDir::new().unwrap();
    let program = stub_program(dir.path());
    let runtime =
        crate::cli_tools::runtime::DelegateRuntime::for_stub("codex", program, dir.path());

    let models = list_models(runtime, br#"{"auth":"stub"}"#.to_vec())
        .await
        .expect("the stub answers");

    let ids: Vec<_> = models.iter().map(|m| m.remote_id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
}

// haex-mail's tools under the app's name and under any other (follow-up of spec 082).
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::mail_names::{MailNames, SET_MAIL_PREFIX};
use super::runner::{run_everyday, Profile};
use super::test_support::{request_with, tool_results};
use super::{embedded_mail_tools, embedded_mail_tools_as, embedded_world};
use crate::adapters::types::StreamChunk;
use crate::chat::eval::embedded_tools;
use crate::chat::eval::testing::{calls, Scripted};

#[test]
fn the_app_names_haex_mail_s_tools_after_the_extension() {
    let mail = embedded_mail_tools();
    assert!(mail
        .iter()
        .all(|def| def.model_name.starts_with("x_haexmail_")));
    let names = MailNames::of(&mail);
    assert_eq!(
        names.in_set("x_haexmail_find_messages"),
        "x_mail_find_messages"
    );
    assert_eq!(
        names.in_app("x_mail_find_messages"),
        "x_haexmail_find_messages"
    );
    // Every other tool keeps its name both ways.
    assert_eq!(names.in_set("contacts_search"), "contacts_search");
    assert_eq!(names.in_app("contacts_search"), "contacts_search");
}

#[test]
fn a_mail_client_called_anything_has_the_same_tools() {
    let thunderbird = embedded_mail_tools_as("thunderbird", "Thunderbird");
    let haex_mail = embedded_mail_tools();
    assert_eq!(thunderbird.len(), haex_mail.len());
    for (renamed, original) in thunderbird.iter().zip(&haex_mail) {
        assert!(renamed.model_name.starts_with("x_thunderbird_"));
        assert_eq!(renamed.tool, original.tool);
        assert_eq!(renamed.extension_title, "Thunderbird");
    }
    let names = MailNames::of(&thunderbird);
    assert!(thunderbird
        .iter()
        .all(|def| names.in_set(&def.model_name).starts_with(SET_MAIL_PREFIX)));
}

#[tokio::test]
async fn a_run_scores_the_app_s_names_by_the_set_s() {
    // The model calls the tools as offered; the trace and the world speak the set's names.
    let model = Scripted::new(|req| match tool_results(req).len() {
        0 => vec![calls(vec![(
            "x_thunderbird_find_messages",
            json!({ "from": "bernd" }),
        )])],
        _ => vec![StreamChunk::Delta {
            content: "Bernd schreibt wegen der Rechnung.".to_owned(),
            reasoning: None,
        }],
    });
    let runs = run_everyday(
        &model,
        "m",
        true,
        &request_with(
            "Was schreibt Bernd?",
            json!({ "calls": [{ "tool": "x_mail_find_messages" }] }),
        ),
        &embedded_tools(),
        &embedded_mail_tools_as("thunderbird", "Thunderbird"),
        &embedded_world(),
        Profile::Desktop,
        None,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let call = &runs[0].trace.calls[0];
    assert_eq!(call.tool, "x_mail_find_messages");
    assert!(!call.result.as_ref().unwrap().is_error, "{call:?}");
}

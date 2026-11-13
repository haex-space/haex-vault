//! A pass seen through the password functions of an extension (spec 092, FR-027, SC-004): it is
//! listed only when the extension asks for passes, the list row has no code, and the extension can
//! neither read, change nor delete it. The rules themselves are tested in
//! `tests/passwords_pass_access.rs`; here: that this adapter hands them the right caller.

use serde_json::{json, Value};

use super::block_on;
use super::tests::setup;
use crate::passwords::access::Caller;
use crate::passwords::pass::model::{CodeInput, PassInput};
use crate::passwords::service::PasswordsService;

/// A code value no answer of the adapter may carry.
const CODE_MARKER: &str = "SECRET-MARKER-extension-code";

fn ids(listed: &Value) -> Vec<String> {
    listed
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn an_extension_lists_a_pass_only_on_request_and_never_reads_changes_or_deletes_it() {
    let s = setup();
    s.permit("readWrite", "haex-calendar", "granted");
    let code: CodeInput =
        serde_json::from_value(json!({ "format": "qr", "value": CODE_MARKER, "position": 0 }))
            .unwrap();
    let pass = block_on(PasswordsService::new(s.vault.clone()).pass_create(
        &Caller::User,
        PassInput {
            title: Some("Concert".to_owned()),
            tag_names: vec!["haex-calendar".to_owned()],
            codes: vec![code],
            ..PassInput::default()
        },
        None,
    ))
    .unwrap()
    .header
    .id;

    let logins = s.call("extension_password_list", json!({})).unwrap();
    assert_eq!(ids(&logins), [s.calendar.as_str()], "the default is logins");

    let passes = s
        .call("extension_password_list", json!({ "type": "pass" }))
        .unwrap();
    assert_eq!(ids(&passes), [pass.as_str()]);
    assert_eq!(passes[0]["type"], "pass");
    let any = s
        .call("extension_password_list", json!({ "type": "any" }))
        .unwrap();
    assert_eq!(ids(&any).len(), 2);
    for listed in [&passes, &any] {
        assert!(!listed.to_string().contains(CODE_MARKER), "{listed}");
    }

    for (method, params) in [
        ("extension_password_read", json!({ "itemId": pass })),
        (
            "extension_password_update",
            json!({ "itemId": pass, "input": { "title": "Changed" } }),
        ),
        ("extension_password_delete", json!({ "itemId": pass })),
    ] {
        let answer = s.call(method, params);
        let error = answer.expect_err(method);
        assert!(
            !format!("{error:?}").contains(CODE_MARKER),
            "{method} answered with a code"
        );
    }
}

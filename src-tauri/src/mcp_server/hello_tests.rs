use serde_json::json;

use super::hello::{
    control_line, json_line, parse_greeting, read_control, valid_slug, CloseReason, Control,
    Greeting, HelloError, HelloReply, RejectReason, MAX_GREETING_BYTES,
};

fn greeting(value: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&value).expect("json")
}

#[test]
fn a_greeting_of_the_contract_is_read() {
    let parsed = parse_greeting(&greeting(json!({
        "hello": 1, "slug": "claude-code", "token": null, "delegate": false
    })))
    .expect("valid");
    assert_eq!(
        parsed,
        Greeting {
            hello: 1,
            slug: "claude-code".to_owned(),
            token: None,
            delegate: false,
        }
    );
    let with_token = parse_greeting(&greeting(
        json!({ "hello": 1, "slug": "codex", "token": "abc" }),
    ))
    .expect("delegate defaults to false");
    assert_eq!(with_token.token.as_deref(), Some("abc"));
    assert!(!with_token.delegate);
}

#[test]
fn a_bad_greeting_is_refused() {
    assert_eq!(
        parse_greeting(&vec![b' '; MAX_GREETING_BYTES + 1]),
        Err(HelloError::TooLong)
    );
    assert_eq!(parse_greeting(b"not json"), Err(HelloError::Malformed));
    assert_eq!(
        parse_greeting(&greeting(json!({ "hello": 1, "token": null }))),
        Err(HelloError::Malformed),
        "the slug is missing"
    );
    assert_eq!(
        parse_greeting(&greeting(
            json!({ "hello": 1, "slug": "x", "token": null, "extra": 1 })
        )),
        Err(HelloError::Malformed)
    );
    assert_eq!(
        parse_greeting(&greeting(json!({ "hello": 2, "slug": "x", "token": null }))),
        Err(HelloError::Version)
    );
    for slug in ["", "Claude", "a b", "ä", &"a".repeat(33), "../x"] {
        assert_eq!(
            parse_greeting(&greeting(
                json!({ "hello": 1, "slug": slug, "token": null })
            )),
            Err(HelloError::Slug),
            "{slug:?}"
        );
    }
    assert!(valid_slug("a-1"));
    assert!(valid_slug(&"a".repeat(32)));
}

#[test]
fn replies_have_the_shape_of_the_contract() {
    assert_eq!(
        json_line(&HelloReply::Accepted),
        "{\"status\":\"accepted\"}\n"
    );
    assert_eq!(
        json_line(&HelloReply::Pending),
        "{\"status\":\"pending\"}\n"
    );
    assert_eq!(
        json_line(&HelloReply::Rejected {
            reason: RejectReason::RegistrationBusy
        }),
        "{\"status\":\"rejected\",\"reason\":\"registration_busy\"}\n"
    );
}

#[test]
fn control_lines_round_trip_and_never_look_like_json_rpc() {
    let token = control_line(Control::Token("t0k".to_owned()));
    assert_eq!(token, "{\"holzi_control\":{\"token\":\"t0k\"}}\n");
    assert_eq!(
        read_control(token.trim_end().as_bytes()),
        Some(Control::Token("t0k".to_owned()))
    );
    let closing = control_line(Control::Closing(CloseReason::Revoked));
    assert_eq!(
        read_control(closing.trim_end().as_bytes()),
        Some(Control::Closing(CloseReason::Revoked))
    );
    for message in [
        json!({ "jsonrpc": "2.0", "id": 1, "result": {} }),
        json!({ "jsonrpc": "2.0", "method": "notifications/tools/list_changed" }),
        json!({ "holzi_control": { "token": "x" }, "jsonrpc": "2.0" }),
        json!({ "holzi_control": { "unknown": 1 } }),
    ] {
        assert_eq!(
            read_control(&serde_json::to_vec(&message).expect("json")),
            None,
            "{message}"
        );
    }
}

#[test]
fn debug_output_never_shows_a_credential() {
    let greeting = Greeting {
        hello: 1,
        slug: "codex".to_owned(),
        token: Some("secret-credential".to_owned()),
        delegate: false,
    };
    assert!(!format!("{greeting:?}").contains("secret-credential"));
    assert!(
        !format!("{:?}", Control::Token("secret-credential".to_owned()))
            .contains("secret-credential")
    );
}

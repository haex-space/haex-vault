use serde_json::json;
use uuid::Uuid;

use super::dispatch::{call, tests::setup_with};
use crate::openers::mail::MailRequest;

const MAIL_OPENER: &str = "extension_e2e/mail-opener.xt";
const NOTES_LIKE: &str = "extension_bundles/good-notes-like.xt";

fn request(subject: &str) -> MailRequest {
    MailRequest {
        to: vec!["ben@example.org".to_owned()],
        subject: subject.to_owned(),
        ..MailRequest::default()
    }
}

#[test]
fn an_extension_takes_its_own_request_once_without_any_permission() {
    let s = setup_with(MAIL_OPENER);
    s.ctx
        .host
        .openers
        .park(s.ctx.session.extension_id, request("Hallo"));
    let taken = call(&s.ctx, "extension_opener_take", &json!(null)).unwrap();
    assert_eq!(
        taken,
        json!({
            "action": "mail.compose",
            "mail": {
                "to": ["ben@example.org"],
                "cc": [],
                "bcc": [],
                "subject": "Hallo",
                "body": "",
            }
        })
    );
    assert_eq!(
        call(&s.ctx, "extension_opener_take", &json!(null)).unwrap(),
        json!(null)
    );
}

#[test]
fn nothing_waiting_is_null() {
    let s = setup_with(MAIL_OPENER);
    assert_eq!(
        call(&s.ctx, "extension_opener_take", &json!(null)).unwrap(),
        json!(null)
    );
}

#[test]
fn an_extension_does_not_get_the_request_of_another() {
    let s = setup_with(MAIL_OPENER);
    s.ctx
        .host
        .openers
        .park(Uuid::new_v4(), request("not yours"));
    assert_eq!(
        call(&s.ctx, "extension_opener_take", &json!(null)).unwrap(),
        json!(null)
    );
}

#[test]
fn an_extension_that_did_not_register_gets_null_and_leaves_the_request_parked() {
    let s = setup_with(NOTES_LIKE);
    let id = s.ctx.session.extension_id;
    s.ctx.host.openers.park(id, request("Hallo"));
    assert_eq!(
        call(&s.ctx, "extension_opener_take", &json!(null)).unwrap(),
        json!(null)
    );
    assert_eq!(s.ctx.host.openers.take(id), Some(request("Hallo")));
}

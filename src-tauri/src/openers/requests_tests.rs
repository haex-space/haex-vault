use std::sync::Mutex;
use std::thread::sleep;
use std::time::Instant;

use super::*;
use crate::extensions::bridge::events::FRAME_EVENT;

#[derive(Default)]
struct Recorded(Mutex<Vec<(String, Value)>>);

impl Emit for Recorded {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorded {
    fn events(&self) -> Vec<(String, Value)> {
        self.0.lock().unwrap().clone()
    }

    fn wait_for(&self, event: &str) -> Value {
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            if let Some((_, payload)) = self.events().into_iter().find(|(name, _)| name == event) {
                return payload;
            }
            sleep(Duration::from_millis(5));
        }
        panic!("no {event} came");
    }
}

fn mail(subject: &str) -> MailRequest {
    MailRequest {
        to: vec!["ben@example.org".to_owned()],
        subject: subject.to_owned(),
        ..MailRequest::default()
    }
}

#[test]
fn a_parked_request_is_taken_exactly_once() {
    let requests = OpenRequests::default();
    let ext = Uuid::new_v4();
    requests.park(ext, mail("Hallo"));
    assert_eq!(requests.take(ext), Some(mail("Hallo")));
    assert_eq!(requests.take(ext), None);
}

#[test]
fn a_new_request_replaces_the_old_one_of_the_same_extension() {
    let requests = OpenRequests::default();
    let ext = Uuid::new_v4();
    requests.park(ext, mail("first"));
    requests.park(ext, mail("second"));
    assert_eq!(requests.take(ext), Some(mail("second")));
    assert_eq!(requests.take(ext), None);
}

#[test]
fn another_extension_takes_nothing() {
    let requests = OpenRequests::default();
    let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
    requests.park(mine, mail("Hallo"));
    assert_eq!(requests.take(other), None);
    assert_eq!(requests.take(mine), Some(mail("Hallo")));
}

#[test]
fn an_expiry_drops_only_the_request_it_belongs_to() {
    let requests = OpenRequests::default();
    let ext = Uuid::new_v4();
    let old = requests.park(ext, mail("old"));
    requests.park(ext, mail("new"));
    assert!(!requests.expire(ext, old));
    assert_eq!(requests.take(ext), Some(mail("new")));
}

#[test]
fn a_request_nobody_takes_expires_and_the_window_hears_of_it() {
    let host = Arc::new(ExtensionHost::default());
    let recorded = Arc::new(Recorded::default());
    let ext = Uuid::new_v4();
    park_and_announce(
        &host,
        Arc::clone(&recorded) as Arc<dyn Emit>,
        ext,
        mail("Hallo"),
        Duration::from_millis(20),
    );
    let payload = recorded.wait_for(EXPIRED_EVENT);
    assert_eq!(payload["extensionId"], ext.to_string());
    assert_eq!(host.openers.take(ext), None);
}

#[test]
fn a_request_taken_in_time_does_not_expire() {
    let host = Arc::new(ExtensionHost::default());
    let recorded = Arc::new(Recorded::default());
    let ext = Uuid::new_v4();
    park_and_announce(
        &host,
        Arc::clone(&recorded) as Arc<dyn Emit>,
        ext,
        mail("Hallo"),
        Duration::from_millis(20),
    );
    assert_eq!(host.openers.take(ext), Some(mail("Hallo")));
    sleep(Duration::from_millis(100));
    assert!(recorded.events().is_empty());
}

#[test]
fn every_open_frame_of_the_extension_is_told_without_content() {
    let host = Arc::new(ExtensionHost::default());
    let recorded = Arc::new(Recorded::default());
    let ext = Uuid::new_v4();
    host.frames.open(ext, Uuid::new_v4(), "tab-1");
    host.frames.open(ext, Uuid::new_v4(), "tab-2");
    host.frames.open(Uuid::new_v4(), Uuid::new_v4(), "tab-3");
    park_and_announce(
        &host,
        Arc::clone(&recorded) as Arc<dyn Emit>,
        ext,
        mail("secret"),
        TAKE_TIMEOUT,
    );
    let told = recorded.events();
    assert_eq!(told.len(), 2);
    for (name, payload) in told {
        assert_eq!(name, FRAME_EVENT);
        assert_eq!(payload["type"], REQUEST_EVENT);
        assert_eq!(payload["data"], json!({}));
    }
}

#[test]
fn the_answer_names_the_action_and_carries_the_fields_as_text() {
    let answer = answer_of(&MailRequest {
        to: vec!["a@example.org".to_owned()],
        cc: vec!["b@example.org".to_owned()],
        bcc: Vec::new(),
        subject: "Hi".to_owned(),
        body: "Text\nzwei".to_owned(),
    });
    assert_eq!(
        answer,
        json!({
            "action": "mail.compose",
            "mail": {
                "to": ["a@example.org"],
                "cc": ["b@example.org"],
                "bcc": [],
                "subject": "Hi",
                "body": "Text\nzwei",
            }
        })
    );
}

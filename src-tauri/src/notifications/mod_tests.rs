use std::sync::{Arc, Mutex};

use super::*;

#[test]
fn a_chat_notification_opens_its_conversation_only_on_a_tap() {
    let chat = || Target::Chat {
        thread_id: "t1".to_owned(),
    };
    assert_eq!(
        chat().answer(NotificationResponse::Body),
        Some("t1".to_owned())
    );
    assert_eq!(chat().answer(NotificationResponse::Closed), None);
    assert_eq!(
        chat().answer(NotificationResponse::Button("x".to_owned())),
        None
    );
}

#[test]
fn an_extension_notification_hands_every_response_on() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    for response in [
        NotificationResponse::Body,
        NotificationResponse::Button("ok".to_owned()),
        NotificationResponse::Closed,
    ] {
        let into = Arc::clone(&heard);
        let target = Target::Respond(Box::new(move |r| into.lock().unwrap().push(r)));
        assert_eq!(target.answer(response), None);
    }
    assert_eq!(heard.lock().unwrap().len(), 3);
}

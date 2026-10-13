use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use uuid::Uuid;

use super::frame_request::{FrameRequests, AGENT_FRAME};
use crate::extensions::bridge::dispatch::Emit;

#[derive(Clone, Default)]
struct Recorded(Arc<Mutex<Vec<(String, Value)>>>);

impl Emit for Recorded {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorded {
    async fn request_id(&self) -> String {
        for _ in 0..100 {
            if let Some((_, payload)) = self.0.lock().unwrap().first() {
                return payload["requestId"].as_str().unwrap().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("no frame request was sent");
    }
}

#[tokio::test]
async fn holzis_window_is_asked_for_a_frame_and_its_answer_returns() {
    let requests = Arc::new(FrameRequests::default());
    let recorded = Recorded::default();
    let extension_id = Uuid::new_v4();
    let (asking, emit) = (Arc::clone(&requests), recorded.clone());
    let pending = tokio::spawn(async move {
        asking
            .request(&emit, extension_id, Duration::from_secs(5))
            .await
    });

    let request_id = recorded.request_id().await;
    assert_eq!(
        recorded.0.lock().unwrap()[0],
        (
            AGENT_FRAME.to_owned(),
            json!({ "extensionId": extension_id.to_string(), "requestId": request_id })
        )
    );
    assert!(requests.answer(&request_id, Some("frame-1".to_owned())));
    assert_eq!(pending.await.unwrap().as_deref(), Some("frame-1"));
    assert!(
        !requests.answer(&request_id, None),
        "a request is answered once"
    );
}

#[tokio::test]
async fn a_window_that_could_not_open_the_extension_answers_none() {
    let requests = Arc::new(FrameRequests::default());
    let recorded = Recorded::default();
    let (asking, emit) = (Arc::clone(&requests), recorded.clone());
    let pending = tokio::spawn(async move {
        asking
            .request(&emit, Uuid::new_v4(), Duration::from_secs(5))
            .await
    });
    let request_id = recorded.request_id().await;
    assert!(requests.answer(&request_id, None));
    assert_eq!(pending.await.unwrap(), None);
}

#[tokio::test]
async fn no_answer_in_time_is_none_and_a_late_answer_is_dropped() {
    let requests = FrameRequests::default();
    let recorded = Recorded::default();
    let frame = requests
        .request(&recorded, Uuid::new_v4(), Duration::from_millis(20))
        .await;
    assert_eq!(frame, None);
    let request_id = recorded.request_id().await;
    assert!(!requests.answer(&request_id, Some("late".to_owned())));
}

#[test]
fn an_unknown_request_is_ignored() {
    assert!(!FrameRequests::default().answer("nope", Some("frame".to_owned())));
}

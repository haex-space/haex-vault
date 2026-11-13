//! The open questions of `ModelReplacements`: an answer reaches the waiting job, and a refusal, a
//! cancelled job or no answer in time end the wait and leave nothing open.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::*;

#[derive(Clone, Default)]
struct Recorded(Arc<Mutex<Vec<(String, Value)>>>);

impl Emit for Recorded {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorded {
    fn events(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|(e, _)| e.clone())
            .collect()
    }

    async fn request_id(&self) -> String {
        for _ in 0..200 {
            if let Some((event, payload)) = self.0.lock().unwrap().first() {
                assert_eq!(event, REPLACEMENT_EVENT);
                return payload["requestId"].as_str().unwrap().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("no question was sent");
    }
}

fn request() -> ModelReplacementRequest {
    ModelReplacementRequest {
        request_id: String::new(),
        extension_id: Uuid::new_v4().to_string(),
        display_name: "Notes".into(),
        failed_model: "broken".into(),
        failed_was_default: true,
        outcome: "unusable".into(),
        reason: None,
        models: Vec::new(),
        transfer: ExtensionModelTransfer {
            kinds: vec!["text".into()],
            text_bytes: 1,
            image_count: 0,
            image_bytes: 0,
            earlier_result: false,
        },
    }
}

fn pick() -> ModelReplacementAnswer {
    ModelReplacementAnswer {
        model_id: "m-b".into(),
        effort: "none".into(),
        make_default: true,
    }
}

fn open(replacements: &ModelReplacements) -> usize {
    replacements.pending().len()
}

#[tokio::test]
async fn the_answer_of_the_window_reaches_the_waiting_job() {
    let replacements = Arc::new(ModelReplacements::default());
    let recorded = Recorded::default();
    let (asking, emit) = (Arc::clone(&replacements), recorded.clone());
    let waiting = tokio::spawn(async move {
        asking
            .ask(&emit, request(), &CancellationToken::new())
            .await
    });
    let id = recorded.request_id().await;
    assert!(replacements.answer(&id, Some(pick())));
    assert_eq!(waiting.await.unwrap(), Some(pick()));
    assert_eq!(open(&replacements), 0);
    assert!(
        !replacements.answer(&id, Some(pick())),
        "a second answer is late"
    );
}

#[tokio::test]
async fn a_refusal_ends_the_wait_without_an_answer() {
    let replacements = Arc::new(ModelReplacements::default());
    let recorded = Recorded::default();
    let (asking, emit) = (Arc::clone(&replacements), recorded.clone());
    let waiting = tokio::spawn(async move {
        asking
            .ask(&emit, request(), &CancellationToken::new())
            .await
    });
    let id = recorded.request_id().await;
    assert!(replacements.answer(&id, None));
    assert_eq!(waiting.await.unwrap(), None);
    assert_eq!(open(&replacements), 0);
}

#[tokio::test]
async fn no_answer_in_time_ends_the_wait_and_a_late_answer_is_not_an_error() {
    let replacements = ModelReplacements::default();
    replacements.set_timeout(Duration::from_millis(30));
    let recorded = Recorded::default();
    let answer = replacements
        .ask(&recorded, request(), &CancellationToken::new())
        .await;
    assert_eq!(answer, None);
    assert_eq!(open(&replacements), 0, "nothing stays open");
    assert_eq!(
        recorded.events(),
        [REPLACEMENT_EVENT, REPLACEMENT_ENDED_EVENT],
        "the window is told the question is moot"
    );
    let id = recorded.request_id().await;
    assert!(!replacements.answer(&id, Some(pick())));
    assert!(!replacements.answer("unknown", Some(pick())));
}

#[tokio::test]
async fn a_cancelled_job_ends_the_wait() {
    let replacements = Arc::new(ModelReplacements::default());
    let recorded = Recorded::default();
    let cancel = CancellationToken::new();
    let (asking, emit, token) = (Arc::clone(&replacements), recorded.clone(), cancel.clone());
    let waiting = tokio::spawn(async move { asking.ask(&emit, request(), &token).await });
    recorded.request_id().await;
    cancel.cancel();
    assert_eq!(waiting.await.unwrap(), None);
    assert_eq!(open(&replacements), 0);
    assert_eq!(
        recorded.events(),
        [REPLACEMENT_EVENT, REPLACEMENT_ENDED_EVENT]
    );
}

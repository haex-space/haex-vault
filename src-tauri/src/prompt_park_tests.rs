//! Tests for the parked questions (spec 049, T014): one answer per question, timeout, cancel and
//! the missing window.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::{PromptEnd, PromptPark};

/// What the emitter saw: the id and the question of each.
type Seen = Arc<Mutex<Vec<(String, String)>>>;

fn park() -> (PromptPark<String, u32>, Seen) {
    let park = PromptPark::new(Duration::from_secs(120));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    park.set_emitter(Arc::new(move |id: &str, q: &String| {
        log.lock().expect("log").push((id.to_owned(), q.clone()));
        true
    }));
    (park, seen)
}

#[tokio::test(start_paused = true)]
async fn an_answer_arrives_once() {
    let (park, seen) = park();
    let token = CancellationToken::new();
    let asker = {
        let park = park.clone();
        let token = token.clone();
        tokio::spawn(async move { park.ask(&"allow?".to_owned(), &token, |_| {}).await })
    };
    while seen.lock().expect("log").is_empty() {
        tokio::task::yield_now().await;
    }
    let id = seen.lock().expect("log")[0].0.clone();
    assert!(park.answer(&id, 7));
    assert!(!park.answer(&id, 8), "a second answer is ignored");
    assert_eq!(asker.await.expect("join"), Ok(7));
    assert_eq!(park.waiting(), 0);
}

#[tokio::test(start_paused = true)]
async fn nobody_answering_expires() {
    let (park, _) = park();
    let token = CancellationToken::new();
    assert_eq!(
        park.ask(&"q".to_owned(), &token, |_| {}).await,
        Err(PromptEnd::Expired)
    );
    assert_eq!(park.waiting(), 0);
}

#[tokio::test(start_paused = true)]
async fn the_token_cancels() {
    let (park, _) = park();
    let token = CancellationToken::new();
    let child = token.child_token();
    let asker = {
        let park = park.clone();
        tokio::spawn(async move { park.ask(&"q".to_owned(), &child, |_| {}).await })
    };
    tokio::task::yield_now().await;
    token.cancel();
    assert_eq!(asker.await.expect("join"), Err(PromptEnd::Cancelled));
}

#[tokio::test(start_paused = true)]
async fn cancel_all_ends_every_waiter() {
    let (park, seen) = park();
    let token = CancellationToken::new();
    let mut askers = Vec::new();
    for _ in 0..3 {
        let park = park.clone();
        let token = token.clone();
        askers.push(tokio::spawn(async move {
            park.ask(&"q".to_owned(), &token, |_| {}).await
        }));
    }
    while seen.lock().expect("log").len() < 3 {
        tokio::task::yield_now().await;
    }
    park.cancel_all();
    for asker in askers {
        assert_eq!(asker.await.expect("join"), Err(PromptEnd::Cancelled));
    }
}

#[tokio::test]
async fn without_a_window_nobody_is_asked() {
    let park: PromptPark<String, u32> = PromptPark::new(Duration::from_secs(1));
    let token = CancellationToken::new();
    assert_eq!(
        park.ask(&"q".to_owned(), &token, |_| {}).await,
        Err(PromptEnd::NoWindow)
    );
    let refusing: PromptPark<String, u32> = PromptPark::new(Duration::from_secs(1));
    refusing.set_emitter(Arc::new(|_: &str, _: &String| false));
    assert_eq!(
        refusing.ask(&"q".to_owned(), &token, |_| {}).await,
        Err(PromptEnd::NoWindow)
    );
    assert!(!refusing.answer("unknown", 1));
}

#[tokio::test(start_paused = true)]
async fn on_id_learns_the_id_before_the_question_goes_out() {
    let (park, seen) = park();
    let token = CancellationToken::new();
    let learned = Arc::new(Mutex::new(String::new()));
    let asker = {
        let park = park.clone();
        let learned = Arc::clone(&learned);
        tokio::spawn(async move {
            park.ask(&"q".to_owned(), &token, |id| {
                *learned.lock().expect("id") = id.to_owned();
            })
            .await
        })
    };
    while seen.lock().expect("log").is_empty() {
        tokio::task::yield_now().await;
    }
    let id = seen.lock().expect("log")[0].0.clone();
    assert_eq!(*learned.lock().expect("id"), id);
    park.answer(&id, 1);
    assert_eq!(asker.await.expect("join"), Ok(1));
}

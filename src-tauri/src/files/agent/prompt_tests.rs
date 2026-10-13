use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::{
    FilesAgentAnswer, FilesAgentChoice, FilesAgentPermissionRequest, FilesAgentWant,
    PermissionPrompt,
};

/// A prompt whose window records each question and, with `reply`, answers it at once.
fn prompt_with_window(
    reply: Option<FilesAgentChoice>,
    timeout: Duration,
) -> (
    PermissionPrompt,
    Arc<Mutex<Vec<FilesAgentPermissionRequest>>>,
) {
    let prompt = PermissionPrompt::with_timeout(timeout);
    let asked = Arc::new(Mutex::new(Vec::new()));
    let (seen, answering) = (asked.clone(), prompt.clone());
    prompt.set_emitter(Arc::new(move |request| {
        seen.lock().expect("seen").push(request.clone());
        if let Some(choice) = reply {
            let (answering, id) = (answering.clone(), request.request_id.clone());
            tokio::spawn(async move {
                answering.answer(&id, FilesAgentAnswer::plain(choice));
            });
        }
        true
    }));
    (prompt, asked)
}

async fn ask(prompt: &PermissionPrompt, cancel: &CancellationToken) -> Option<FilesAgentChoice> {
    prompt
        .ask("builtin", "s1", "Fotos", FilesAgentWant::Read, None, cancel)
        .await
        .map(|answer| answer.choice)
}

#[tokio::test]
async fn the_answer_of_the_window_comes_back() {
    let (prompt, asked) =
        prompt_with_window(Some(FilesAgentChoice::ReadWrite), Duration::from_secs(5));
    assert_eq!(
        ask(&prompt, &CancellationToken::new()).await,
        Some(FilesAgentChoice::ReadWrite)
    );
    let asked = asked.lock().expect("asked");
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].storage_name, "Fotos");
    assert_eq!(asked[0].wants, FilesAgentWant::Read);
}

#[tokio::test]
async fn without_a_window_nobody_is_asked() {
    let prompt = PermissionPrompt::with_timeout(Duration::from_secs(5));
    assert_eq!(ask(&prompt, &CancellationToken::new()).await, None);
    prompt.set_emitter(Arc::new(|_| false));
    assert_eq!(ask(&prompt, &CancellationToken::new()).await, None);
}

#[tokio::test]
async fn no_answer_in_time_refuses_and_a_late_answer_finds_nothing() {
    let (prompt, asked) = prompt_with_window(None, Duration::from_millis(20));
    assert_eq!(ask(&prompt, &CancellationToken::new()).await, None);
    let id = asked.lock().expect("asked")[0].request_id.clone();
    assert!(!prompt.answer(&id, FilesAgentAnswer::plain(FilesAgentChoice::Read)));
}

#[tokio::test]
async fn a_cancelled_turn_stops_waiting() {
    let (prompt, _asked) = prompt_with_window(None, Duration::from_secs(60));
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(ask(&prompt, &cancel).await, None);
}

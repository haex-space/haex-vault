//! The invitation prompt (spec 057 T034, FR-014, FR-015, SC-006): a window prompt while a frame of
//! the extension is open, a notification with two buttons otherwise, the window again where the
//! system shows none, and the prompt goes when the invitation does.

use std::sync::{Arc, Mutex, PoisonError};

use super::*;
use crate::extensions::peers::test_support::{Admit, Vault};

fn info(vault: &Vault) -> Arc<PromptInfo> {
    Arc::new(PromptInfo {
        invite: Uuid::new_v4(),
        extension_id: vault.extension_id,
        extension_name: "Doom".to_owned(),
        contact_name: "Ana".to_owned(),
    })
}

/// An answer that writes down what the notification's buttons said.
fn recorder() -> (Answer, Arc<Mutex<Vec<bool>>>) {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let answer: Answer = Arc::new(move |accept| {
        sink.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(accept);
    });
    (answer, heard)
}

fn heard(heard: &Mutex<Vec<bool>>) -> Vec<bool> {
    heard.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_an_open_frame_the_prompt_is_in_the_window() {
    let vault = Vault::start(Admit::AtOnce).await;
    let info = info(&vault);
    let (answer, _) = recorder();
    let _prompt = present(&vault.host, &vault.ctx.frames, Arc::clone(&info), answer).await;

    let events = vault.window.window_events(INVITATION);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["inviteId"], info.invite.to_string());
    assert_eq!(events[0]["extensionId"], vault.extension_id.to_string());
    assert_eq!(events[0]["extensionName"], "Doom");
    assert_eq!(events[0]["contactName"], "Ana");
    assert_eq!(events[0]["raise"], false);
    assert!(vault.desktop.specs().is_empty());
    // The extension itself hears nothing (SC-006).
    assert_eq!(vault.window.all_frame_events(), 0);
    vault.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_frame_the_prompt_is_a_notification_with_two_buttons() {
    let vault = Vault::start(Admit::AtOnce).await;
    vault.close_frames();
    let (answer, answers) = recorder();
    let _prompt = present(&vault.host, &vault.ctx.frames, info(&vault), answer).await;

    assert!(vault.window.window_events(INVITATION).is_empty());
    let shown = vault.desktop.specs();
    assert_eq!(shown.len(), 1);
    let spec = &shown[0];
    assert!(vault.desktop.closed().is_empty());
    assert_eq!(spec.title, "Ana lädt dich zu „Doom“ ein");
    let buttons: Vec<(&str, &str)> = spec
        .buttons
        .iter()
        .map(|b| (b.id.as_str(), b.label.as_str()))
        .collect();
    assert_eq!(buttons, [("accept", "Annehmen"), ("decline", "Ablehnen")]);

    vault
        .desktop
        .answer(0, NotificationResponse::Button("accept".to_owned()));
    assert_eq!(heard(&answers), [true]);
    vault.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_decline_button_declines() {
    let vault = Vault::start(Admit::AtOnce).await;
    vault.close_frames();
    let (answer, answers) = recorder();
    let _prompt = present(&vault.host, &vault.ctx.frames, info(&vault), answer).await;

    vault
        .desktop
        .answer(0, NotificationResponse::Button("decline".to_owned()));
    assert_eq!(heard(&answers), [false]);
    vault.shutdown().await;
}

/// A click on the notification itself, and a system without buttons, bring the window prompt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_click_on_the_notification_brings_the_window_prompt() {
    let vault = Vault::start(Admit::AtOnce).await;
    vault.close_frames();
    let (answer, answers) = recorder();
    let _prompt = present(&vault.host, &vault.ctx.frames, info(&vault), answer).await;

    vault.desktop.answer(0, NotificationResponse::Body);
    let events = vault.window.window_events(INVITATION);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["raise"], true, "the window comes forward");
    assert!(
        heard(&answers).is_empty(),
        "a click on the body answers nothing"
    );
    vault.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_notification_decides_nothing() {
    let vault = Vault::start(Admit::AtOnce).await;
    vault.close_frames();
    let (answer, answers) = recorder();
    let _prompt = present(&vault.host, &vault.ctx.frames, info(&vault), answer).await;

    vault.desktop.answer(0, NotificationResponse::Closed);
    assert!(heard(&answers).is_empty());
    assert!(vault.window.window_events(INVITATION).is_empty());
    vault.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_system_that_shows_no_notification_gets_the_window_prompt() {
    let vault = Vault::start(Admit::AtOnce).await;
    vault.close_frames();
    vault.desktop.forbid();
    let (answer, _) = recorder();
    let _prompt = present(&vault.host, &vault.ctx.frames, info(&vault), answer).await;

    assert!(vault.desktop.specs().is_empty());
    assert_eq!(vault.window.window_events(INVITATION).len(), 1);
    vault.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gone_invitation_closes_the_notification_and_tells_the_window() {
    let vault = Vault::start(Admit::AtOnce).await;
    vault.close_frames();
    let info = info(&vault);
    let (answer, _) = recorder();
    let mut prompt = present(&vault.host, &vault.ctx.frames, Arc::clone(&info), answer).await;

    prompt.gone(ExtensionPeerInvitationGoneReason::Expired);
    let events = vault.window.window_events(INVITATION_GONE);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["inviteId"], info.invite.to_string());
    assert_eq!(events[0]["reason"], "expired");
    assert_eq!(
        vault.desktop.closed(),
        [0],
        "the notification was taken away"
    );
    vault.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answered_prompt_takes_its_notification_away_when_it_ends() {
    let vault = Vault::start(Admit::AtOnce).await;
    vault.close_frames();
    let (answer, _) = recorder();
    let prompt = present(&vault.host, &vault.ctx.frames, info(&vault), answer).await;

    drop(prompt);
    assert_eq!(vault.desktop.closed(), [0]);
    assert!(vault.window.window_events(INVITATION_GONE).is_empty());
    vault.shutdown().await;
}

#[test]
fn the_texts_exist_in_both_languages() {
    for locale in ["de", "en"] {
        for key in ["title", ACCEPT, DECLINE, "gone"] {
            assert_ne!(text(locale, key, &[]), key, "{locale}: {key}");
        }
    }
    assert_eq!(
        text("en", "title", &[("contact", "Ana"), ("extension", "Doom")]),
        "Ana invites you to “Doom”"
    );
    assert_eq!(
        text("fr", ACCEPT, &[]),
        "Accept",
        "any other language gets English"
    );
    assert_eq!(text("de", "no-such-key", &[]), "no-such-key");
}

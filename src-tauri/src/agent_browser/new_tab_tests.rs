//! Tests for the question before a tab a page opened (spec 065 T053 `newTab`, research R9): Plan
//! refuses it, Auto opens an allowed website without asking and asks for any other, the answer
//! allowing that website once; Manuell always asks; a blocked address never asks.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use super::error::BrowserError;
use super::executor::BrowserEnv;
use super::prompts::{new_browser_prompts, BrowserPromptKind, BrowserQuestion};
use super::session::{Session, MAX_TABS};
use super::sessions::SessionHooks;
use super::sites::{Site, Verdict};
use super::tab::Paused;
use super::StartConfig;
use crate::caller::Caller;
use crate::chat::tools::permission::PermissionMode;
use crate::mcp_server::grants::{BrowserRights, SiteGrant, SiteLevel};

/// An app whose agent may operate `https://example.org`.
struct Env;

#[async_trait]
impl BrowserEnv for Env {
    async fn rights(&self, _caller: &Caller) -> Option<BrowserRights> {
        Some(BrowserRights {
            user_browser: false,
            sites: [(
                Site::parse("https://example.org").unwrap(),
                SiteGrant {
                    level: SiteLevel::Operate,
                    fill_remembered: false,
                },
            )]
            .into_iter()
            .collect(),
        })
    }

    async fn remember(&self, _: &Caller, _: &Site, _: SiteLevel) -> Result<(), String> {
        Ok(())
    }

    async fn agent_name(&self, _caller: &Caller) -> String {
        "Testagent".to_owned()
    }

    fn locale(&self) -> String {
        "en".to_owned()
    }

    async fn start_config(&self) -> Result<StartConfig, BrowserError> {
        std::future::pending().await
    }
}

/// A session in `mode` whose questions are recorded and answered with `allow`.
fn session(mode: PermissionMode, allow: bool) -> (Arc<Session>, Arc<Mutex<Vec<BrowserQuestion>>>) {
    let prompts = new_browser_prompts();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let (recording, answering) = (Arc::clone(&asked), prompts.clone());
    prompts.set_emitter(Arc::new(move |id: &str, question: &BrowserQuestion| {
        recording.lock().unwrap().push(question.clone());
        let decision = if allow { "allow" } else { "deny" };
        let answer = serde_json::from_value(json!({ "decision": decision })).unwrap();
        let (answering, id) = (answering.clone(), id.to_owned());
        tokio::spawn(async move {
            answering.answer(&id, answer);
        });
        true
    }));
    let session = Session::new(
        "s1".to_owned(),
        Caller::ExternalAgent {
            id: "a1".to_owned(),
        },
        Arc::new(Env),
        prompts,
        Arc::new(|_| {}),
        SessionHooks {
            stop: Arc::new(|| {}),
            changed: Arc::new(|| {}),
            gone: Arc::new(|| {}),
        },
    );
    session.set_mode(mode);
    (session, asked)
}

fn kinds(asked: &Mutex<Vec<BrowserQuestion>>) -> Vec<(BrowserPromptKind, Option<String>)> {
    asked
        .lock()
        .unwrap()
        .iter()
        .map(|question| (question.kind, question.site.clone()))
        .collect()
}

#[tokio::test]
async fn concurrent_new_tabs_cannot_reserve_more_than_the_limit() {
    let (session, _) = session(PermissionMode::Auto, true);
    // A tool call reserves before CDP returns its target; popups can arrive during that await.
    let explicit = session.reserve_new_tab(None).unwrap();
    let reservations = (0..MAX_TABS + 4)
        .map(|n| {
            let session = Arc::clone(&session);
            tokio::task::spawn_blocking(move || {
                session.reserve_new_tab(Some(&format!("target-{n}")))
            })
        })
        .collect::<Vec<_>>();
    let mut ids = vec![explicit.clone()];
    for reservation in reservations {
        if let Some(id) = reservation.await.unwrap() {
            ids.push(id);
        }
    }

    assert_eq!(ids.len(), MAX_TABS);
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), MAX_TABS);
    assert!(session.reserve_new_tab(Some("one-too-many")).is_none());
    assert!(session.reserve_new_tab(None).is_none());

    // Registering the real target replaces its pending slot, without freeing a place for a popup.
    let hooks = session.tab_hooks(&explicit).await;
    (hooks.on_target)("explicit-target");
    assert_eq!(
        session.tab_of_target("explicit-target"),
        Some(explicit.clone())
    );
    assert!(session.reserve_new_tab(Some("still-too-many")).is_none());
    assert!(session.reserve_new_tab(None).is_none());

    // Both failed setup and an ordinary close release the same reservation.
    session.forget_targets(&explicit);
    let pending = session.reserve_new_tab(None).unwrap();
    session.forget_targets(&pending);
    assert!(session.reserve_new_tab(Some("replacement-popup")).is_some());
}

#[tokio::test]
async fn a_new_tab_consumes_only_its_opener_submit_grant_after_disarm() {
    let (session, asked) = session(PermissionMode::Auto, false);
    let paused = Paused {
        url: "https://example.org/submit".to_owned(),
        method: "POST".to_owned(),
        document: true,
        body: Some("field=value".to_owned()),
    };

    session.arm("t1");
    let first = session.reserve_submit_for_new_tab("t1").unwrap();
    session.disarm();
    session.arm("t1");
    let second = session.reserve_submit_for_new_tab("t1").unwrap();
    session.disarm();

    assert_eq!(
        session.check_send("t1", &paused).await,
        Verdict::Refuse("user_declined")
    );
    assert_eq!(
        session.check_new_tab_send(Some(first), &paused).await,
        Verdict::Go
    );
    session.finish_pending_new_tab(Some(first));
    assert_eq!(
        session.check_new_tab_send(Some(second), &paused).await,
        Verdict::Go
    );
    session.finish_pending_new_tab(Some(second));
    assert_eq!(
        session.check_new_tab_send(Some(second), &paused).await,
        Verdict::Refuse("user_declined")
    );
    assert_eq!(kinds(&asked).len(), 2);
}

#[tokio::test]
async fn plan_refuses_a_new_tab_without_asking() {
    let (session, asked) = session(PermissionMode::Plan, true);
    assert_eq!(
        session.check_new_tab("https://example.org/a").await,
        Verdict::Refuse("plan_mode")
    );
    assert!(kinds(&asked).is_empty());
}

#[tokio::test]
async fn auto_opens_an_allowed_website_without_asking() {
    let (session, asked) = session(PermissionMode::Auto, false);
    assert_eq!(
        session.check_new_tab("https://example.org/a").await,
        Verdict::Go
    );
    assert!(kinds(&asked).is_empty());
}

#[tokio::test]
async fn auto_asks_for_another_website_and_the_answer_allows_it_once() {
    let (session, asked) = session(PermissionMode::Auto, true);
    assert_eq!(
        session.check_new_tab("https://partner.example/x").await,
        Verdict::Go
    );
    assert_eq!(
        kinds(&asked),
        [(
            BrowserPromptKind::NewTab,
            Some("https://partner.example".to_owned())
        )]
    );
    assert!(session
        .once_allowed_sites()
        .contains(&Site::parse("https://partner.example").unwrap()));
    // One answer: the website now loads in this session without a second question.
    assert_eq!(
        session.check_new_tab("https://partner.example/y").await,
        Verdict::Go
    );
    assert_eq!(
        session.check("https://partner.example/z").await,
        Verdict::Go
    );
    assert_eq!(kinds(&asked).len(), 1);
}

#[tokio::test]
async fn a_declined_new_tab_allows_nothing() {
    let (session, asked) = session(PermissionMode::Auto, false);
    assert_eq!(
        session.check_new_tab("https://partner.example/x").await,
        Verdict::Refuse("user_declined")
    );
    assert_eq!(kinds(&asked).len(), 1);
    assert!(session.once_allowed_sites().is_empty());
}

#[tokio::test]
async fn manual_asks_even_for_an_allowed_website() {
    let (session, asked) = session(PermissionMode::Manual, false);
    assert_eq!(
        session.check_new_tab("https://example.org/a").await,
        Verdict::Refuse("user_declined")
    );
    assert_eq!(
        kinds(&asked),
        [(
            BrowserPromptKind::NewTab,
            Some("https://example.org".to_owned())
        )]
    );
}

#[tokio::test]
async fn a_blocked_address_is_refused_without_asking() {
    let (session, asked) = session(PermissionMode::Manual, true);
    assert_eq!(
        session.check_new_tab("file:///etc/passwd").await,
        Verdict::Refuse("blocked_address")
    );
    assert!(kinds(&asked).is_empty());
}

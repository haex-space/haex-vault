//! Tests for the session registry (spec 065, T054): a session per agent, listed for the window; a
//! stop ends the running call within a second and holds until a new session begins; revoking ends
//! a session, stopped or not; the end of the browser ends its sessions.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::cdp::Connection;
use super::error::BrowserError;
use super::executor::{BrowserEnv, BrowserTools};
use super::prompts::new_browser_prompts;
use super::session::Session;
use super::sessions::{BrowserSessions, NoSession};
use super::sites::Site;
use super::{AgentBrowser, StartConfig};
use crate::caller::Caller;
use crate::chat::tools::native_action::{NativeCall, NativeExecutor};
use crate::chat::tools::permission::PermissionMode;
use crate::mcp_server::grants::{BrowserRights, SiteGrant, SiteLevel};

/// An app whose agent may read `https://example.org` and whose browser never finishes starting.
struct Env;

#[async_trait]
impl BrowserEnv for Env {
    async fn rights(&self, _caller: &Caller) -> Option<BrowserRights> {
        Some(BrowserRights {
            user_browser: false,
            sites: [(
                Site::parse("https://example.org").unwrap(),
                SiteGrant {
                    level: SiteLevel::Read,
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

fn agent() -> Caller {
    Caller::ExternalAgent {
        id: "a1".to_owned(),
    }
}

/// A registry that counts what it tells the window.
fn registry() -> (BrowserSessions, Arc<AtomicUsize>) {
    let sessions = BrowserSessions::default();
    let told = Arc::new(AtomicUsize::new(0));
    let counting = Arc::clone(&told);
    sessions.set_changed(Arc::new(move || {
        counting.fetch_add(1, Ordering::SeqCst);
    }));
    (sessions, told)
}

fn start(sessions: &BrowserSessions, caller: &Caller) -> Result<Arc<Session>, NoSession> {
    sessions.get_or_start(caller, |id, hooks| {
        Session::new(
            id,
            caller.clone(),
            Arc::new(Env),
            new_browser_prompts(),
            Arc::new(|_| {}),
            hooks,
        )
    })
}

async fn eventually(what: &str, done: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{what} did not happen within five seconds");
}

#[test]
fn an_agent_has_one_session_and_the_window_hears_of_it() {
    let (sessions, told) = registry();
    let first = start(&sessions, &agent()).unwrap();
    let again = start(&sessions, &agent()).unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(told.load(Ordering::SeqCst), 1);
    let listed = sessions.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, first.id);
    assert!(!listed[0].builtin);

    let builtin = start(&sessions, &Caller::BuiltinAgent).unwrap();
    assert_ne!(builtin.id, first.id);
    assert_eq!(sessions.list().len(), 2);
}

#[test]
fn only_agents_have_sessions() {
    let (sessions, _) = registry();
    for caller in [
        Caller::User,
        Caller::Extension {
            id: "e1".to_owned(),
        },
    ] {
        assert_eq!(start(&sessions, &caller).err(), Some(NoSession::NotAnAgent));
    }
}

#[tokio::test]
async fn a_stop_holds_until_a_new_session_begins() {
    let (sessions, _) = registry();
    let session = start(&sessions, &agent()).unwrap();
    assert!(sessions.stop(&session.id).await);
    assert!(session.cancel.is_cancelled(), "its questions and call end");
    assert!(sessions.list().is_empty());
    assert_eq!(start(&sessions, &agent()).err(), Some(NoSession::Stopped));
    assert!(!sessions.stop(&session.id).await, "stopped once");

    sessions.resume(&agent());
    let next = start(&sessions, &agent()).unwrap();
    assert_ne!(next.id, session.id);
}

#[tokio::test]
async fn resuming_leaves_a_running_session_alone() {
    let (sessions, _) = registry();
    let session = start(&sessions, &agent()).unwrap();
    sessions.resume(&agent());
    assert_eq!(start(&sessions, &agent()).unwrap().id, session.id);
}

#[tokio::test]
async fn revoking_ends_a_session_stopped_or_not() {
    let (sessions, _) = registry();
    let session = start(&sessions, &agent()).unwrap();
    sessions.end(&agent()).await;
    assert!(session.cancel.is_cancelled());
    assert!(sessions.list().is_empty());

    let stopped = start(&sessions, &agent()).unwrap();
    sessions.stop(&stopped.id).await;
    sessions.end(&agent()).await;
    assert!(
        start(&sessions, &agent()).is_ok(),
        "a new connection begins anew"
    );
}

#[tokio::test]
async fn the_marker_stops_the_session() {
    let (sessions, _) = registry();
    let session = start(&sessions, &agent()).unwrap();
    (session.marker().await.on_stop)();
    eventually("the stop", || sessions.list().is_empty()).await;
    assert_eq!(start(&sessions, &agent()).err(), Some(NoSession::Stopped));
}

#[tokio::test]
async fn the_end_of_the_browser_ends_its_sessions() {
    let (sessions, _) = registry();
    let session = start(&sessions, &agent()).unwrap();
    let (from_browser, browser_writes) = std::io::pipe().unwrap();
    let (_browser_reads, to_browser) = std::io::pipe().unwrap();
    let connection = Arc::new(Connection::new(from_browser, to_browser));
    drop(session.tabs_of(&connection).await);

    // The browser's window closed: its end of the pipe goes.
    drop(browser_writes);
    eventually("the end of the session", || sessions.list().is_empty()).await;
    assert!(session.cancel.is_cancelled());
    assert!(
        start(&sessions, &agent()).is_ok(),
        "the next call begins a new session, not a stopped one"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_ends_the_running_call_within_a_second() {
    if !crate::platform::capabilities().agent_browser {
        return;
    }
    let (sessions, _) = registry();
    let tools = Arc::new(BrowserTools::new(
        Arc::new(Env),
        AgentBrowser::default(),
        new_browser_prompts(),
        Arc::new(|_| {}),
        sessions.clone(),
    ));
    let call = NativeCall {
        caller: agent(),
        mode: PermissionMode::Auto,
    };
    let running = {
        let (tools, call) = (Arc::clone(&tools), call.clone());
        tokio::spawn(async move {
            tools
                .run(
                    "browser.open",
                    json!({ "url": "https://example.org/" }),
                    &call,
                    &CancellationToken::new(),
                )
                .await
        })
    };
    eventually("the session", || !sessions.list().is_empty()).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let id = sessions.list()[0].id.clone();
    let asked = Instant::now();
    assert!(sessions.stop(&id).await);
    let outcome = running.await.unwrap();
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "{:?}",
        asked.elapsed()
    );
    assert_eq!(outcome.unwrap_err().code, "stopped");

    let refused = tools
        .run(
            "browser.tabs.list",
            json!({}),
            &call,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(refused.code, "stopped", "further calls are refused");
}

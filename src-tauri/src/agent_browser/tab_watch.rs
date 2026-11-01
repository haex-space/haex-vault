//! What runs beside each agent tab ([`super::tab`]): it answers the requests the tab holds through
//! its guard, sets up the frames that attach to it with the same checks, and reports what the
//! marker reports and each frame's target to the tab's session.

use std::sync::{Arc, Mutex, PoisonError};

use futures::future::BoxFuture;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;

use super::cdp::{fetch, page, runtime, target, Connection, Event};
use super::marker::{self, MarkerTexts, Signal};
use super::sites::Verdict;
use super::submit;
use super::tab::{Guard, IsPaused, OnSignal, OnTarget, Paused};

/// What [`watch`] reports.
#[derive(Clone)]
pub(super) struct Reports {
    /// The marker's texts and state, for the marker of each new document.
    pub texts: MarkerTexts,
    pub paused: IsPaused,
    pub on_signal: OnSignal,
    pub on_target: OnTarget,
}

/// Answers the paused requests of `session` and sets up the frames that attach to it. A refused
/// document of `main` (the tab's own frame, whose id is the target's) and any refused request that
/// would have changed something fail a tool; a refused frame stays empty. What the marker reports
/// goes to `on_signal`, each frame's target to `on_target`. Boxed, since a frame's own frames are
/// watched by the same function.
pub(super) fn watch(
    connection: Arc<Connection>,
    session: String,
    main: Option<String>,
    mut events: UnboundedReceiver<Event>,
    guard: Guard,
    refused: Arc<Mutex<Option<&'static str>>>,
    reports: Reports,
) -> BoxFuture<'static, ()> {
    Box::pin(async move {
        while let Some(event) = events.recv().await {
            match event.method.as_str() {
                "Fetch.requestPaused" => {
                    let is_main =
                        main.is_some() && event.params["frameId"].as_str() == main.as_deref();
                    let (connection, session, guard) =
                        (Arc::clone(&connection), session.clone(), Arc::clone(&guard));
                    let main = is_main.then(|| Arc::clone(&refused));
                    let any = Arc::clone(&refused);
                    tokio::spawn(async move {
                        let main = main.as_deref();
                        decide(&connection, &session, &event.params, &guard, main, &any).await;
                    });
                }
                "Target.attachedToTarget" => {
                    let Some(child) = event.params["sessionId"].as_str().map(str::to_owned) else {
                        continue;
                    };
                    if let Some(frame) = event.params["targetInfo"]["targetId"].as_str() {
                        (reports.on_target)(frame);
                    }
                    let child_events = connection.subscribe(Some(&child));
                    tokio::spawn(watch(
                        Arc::clone(&connection),
                        child.clone(),
                        None,
                        child_events,
                        Arc::clone(&guard),
                        Arc::clone(&refused),
                        reports.clone(),
                    ));
                    let connection = Arc::clone(&connection);
                    let texts = reports.texts.clone();
                    tokio::spawn(async move {
                        // The frame waits for holzi; its documents are held before it may go on.
                        let _ = fetch::hold_requests(&connection, &child).await;
                        let _ = target::attach_frames(&connection, &child).await;
                        let _ = page::enable(&connection, &child).await;
                        let _ = marker::install(&connection, &child, &texts).await;
                        let _ = runtime::run_if_waiting(&connection, &child).await;
                    });
                }
                "Runtime.bindingCalled" => match Signal::of(&event.params) {
                    // A new document's marker shows the agent steering; the user may have it.
                    Some(Signal::Hello) if (reports.paused)() => {
                        let context = event.params["executionContextId"].as_i64();
                        let (connection, session) = (Arc::clone(&connection), session.clone());
                        let texts = reports.texts.clone();
                        tokio::spawn(async move {
                            let _ =
                                marker::update(&connection, &session, context, &texts, true).await;
                        });
                    }
                    Some(Signal::Hello) | None => {}
                    Some(signal) => (reports.on_signal)(signal),
                },
                _ => {}
            }
        }
    })
}

async fn decide(
    connection: &Connection,
    session: &str,
    params: &Value,
    guard: &Guard,
    main: Option<&Mutex<Option<&'static str>>>,
    any: &Mutex<Option<&'static str>>,
) {
    let Some(request) = params["requestId"].as_str() else {
        return;
    };
    let paused = Paused {
        url: params["request"]["url"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        method: params["request"]["method"]
            .as_str()
            .unwrap_or("GET")
            .to_owned(),
        document: params["resourceType"] == "Document",
        main: main.is_some(),
        body: params["request"]["postData"].as_str().map(str::to_owned),
    };
    let changing = submit::changing(&paused.method);
    // Reading requests of a page need no decision.
    if !paused.document && !changing {
        let _ = fetch::go_on(connection, session, request).await;
        return;
    }
    let code = match guard(paused).await {
        Verdict::Go => {
            let _ = fetch::go_on(connection, session, request).await;
            return;
        }
        Verdict::Refuse(code) => code,
        // The guard resolves its own questions; one that still asks has no answer.
        Verdict::Ask(_) => "site_not_allowed",
    };
    let refused = if changing { Some(any) } else { main };
    if let Some(refused) = refused {
        *refused.lock().unwrap_or_else(PoisonError::into_inner) = Some(code);
    }
    let _ = fetch::block(connection, session, request).await;
}

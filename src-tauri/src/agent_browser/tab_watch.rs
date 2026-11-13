//! What runs beside each agent tab ([`super::tab`]): it answers the requests the tab holds through
//! its guard, sets up the frames that attach to it with the same checks, and reports what the
//! marker reports and each frame's target to the tab's session.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use futures::future::BoxFuture;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;

use super::cdp::{fetch, page, runtime, target, Connection, Event};
use super::filters::{self, LiveFilter, TrackerFilter};
use super::marker::{self, MarkerTexts, Signal};
use super::sites::Verdict;
use super::submit;
use super::tab::{Guard, IsPaused, OnBlocked, OnSignal, OnTarget, Paused};

/// What [`watch`] reports.
#[derive(Clone)]
pub(super) struct Reports {
    /// The marker's texts and state, for the marker of each new document.
    pub texts: MarkerTexts,
    pub paused: IsPaused,
    pub on_signal: OnSignal,
    pub on_target: OnTarget,
    /// The tracker filters; a request they block never leaves the browser (research R16).
    pub filter: Option<LiveFilter>,
    pub on_blocked: OnBlocked,
    /// Called for a page of the tab's own frame the lists block.
    pub on_document_blocked: Option<OnBlocked>,
    /// Whether the consent script runs in the frames too.
    pub consent: bool,
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
        // The address of the document in each frame of this session, the source the tracker
        // filters judge a request by (research R16).
        let mut pages: HashMap<String, String> = HashMap::new();
        while let Some(event) = events.recv().await {
            match event.method.as_str() {
                "Fetch.requestPaused" => {
                    let frame = event.params["frameId"].as_str();
                    let is_main = main.is_some() && frame == main.as_deref();
                    let (connection, session, guard) =
                        (Arc::clone(&connection), session.clone(), Arc::clone(&guard));
                    let main = is_main.then(|| Arc::clone(&refused));
                    let any = Arc::clone(&refused);
                    let blocking =
                        reports
                            .filter
                            .as_ref()
                            .and_then(LiveFilter::get)
                            .map(|filter| {
                                Blocking {
                                    filter,
                                    frame_page: frame.and_then(|frame| pages.get(frame)).cloned(),
                                    on_blocked: Arc::clone(&reports.on_blocked),
                                    on_document_blocked: reports.on_document_blocked.clone(),
                                    // While the user has the browser, nothing fails a tool.
                                    fails_tool: !(reports.paused)(),
                                }
                            });
                    tokio::spawn(async move {
                        let main = main.as_deref();
                        decide(
                            &connection,
                            &session,
                            &event.params,
                            &guard,
                            main,
                            &any,
                            blocking,
                        )
                        .await;
                    });
                }
                "Page.frameNavigated" => {
                    let frame = &event.params["frame"];
                    if let (Some(id), Some(url)) = (frame["id"].as_str(), frame["url"].as_str()) {
                        pages.insert(id.to_owned(), url.to_owned());
                    }
                }
                "Page.frameDetached" => {
                    if let Some(id) = event.params["frameId"].as_str() {
                        pages.remove(id);
                    }
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
                    let all = reports.filter.is_some();
                    let consent = reports.consent;
                    tokio::spawn(async move {
                        // The frame waits for holzi; its documents are held before it may go on.
                        let _ = fetch::hold_requests(&connection, &child, all).await;
                        let _ = target::attach_frames(&connection, &child).await;
                        let _ = page::enable(&connection, &child).await;
                        let _ = marker::install(&connection, &child, &texts).await;
                        if consent {
                            let _ = super::consent::install(&connection, &child).await;
                        }
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

/// The tracker filters, as one paused request meets them.
struct Blocking {
    filter: Arc<TrackerFilter>,
    /// The address of the document in the request's frame, when known.
    frame_page: Option<String>,
    on_blocked: OnBlocked,
    on_document_blocked: Option<OnBlocked>,
    /// A page the lists block fails the agent's tool.
    fails_tool: bool,
}

impl Blocking {
    /// Whether the lists block the paused request; a blocked page of the tab's own frame then
    /// fails the tool with `blocked_by_filter`.
    fn blocks(&self, params: &Value, main: Option<&Mutex<Option<&'static str>>>) -> bool {
        let Some(info) = filters::request_info(params, main.is_some(), self.frame_page.as_deref())
        else {
            return false;
        };
        if !self.filter.blocks(&info) {
            return false;
        }
        (self.on_blocked)(&info.url);
        if let (Some(_), "Document", Some(blocked)) =
            (main, info.kind.as_str(), &self.on_document_blocked)
        {
            blocked(&info.url);
        }
        if let (Some(main), true, true) = (main, info.kind == "Document", self.fails_tool) {
            *main.lock().unwrap_or_else(PoisonError::into_inner) = Some("blocked_by_filter");
        }
        true
    }
}

async fn decide(
    connection: &Connection,
    session: &str,
    params: &Value,
    guard: &Guard,
    main: Option<&Mutex<Option<&'static str>>>,
    any: &Mutex<Option<&'static str>>,
    blocking: Option<Blocking>,
) {
    let Some(request) = params["requestId"].as_str() else {
        return;
    };
    // The filter lists come first, so a blocked tracker never raises a question (FR-045).
    if blocking.is_some_and(|blocking| blocking.blocks(params, main)) {
        let _ = fetch::block(connection, session, request).await;
        return;
    }
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
        has_body: params["request"]["hasPostData"].as_bool().unwrap_or(false),
    };
    let changing = submit::changing(&paused.method);
    // What is held only for the tracker filters (images, scripts, styles, …) needs no decision;
    // a reading fetch or XHR does, since none may reach a blocked website (FR-047).
    if !changing && !fetch::checked(params["resourceType"].as_str().unwrap_or_default()) {
        let _ = fetch::go_on(connection, session, request).await;
        return;
    }
    let document = paused.document;
    let code = match guard(paused).await {
        Verdict::Go => {
            let _ = fetch::go_on(connection, session, request).await;
            return;
        }
        Verdict::Refuse(code) => code,
        // The guard resolves its own questions; one that still asks has no answer.
        Verdict::Ask(_) => "site_not_allowed",
    };
    // A refused reading request fails only in the page; a page or a sending request fails a tool.
    let refused = if changing {
        Some(any)
    } else if document {
        main
    } else {
        None
    };
    if let Some(refused) = refused {
        *refused.lock().unwrap_or_else(PoisonError::into_inner) = Some(code);
    }
    let _ = fetch::block(connection, session, request).await;
}

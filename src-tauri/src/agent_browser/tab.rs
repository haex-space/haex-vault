//! One agent tab (spec 065, contracts/agent-browser.md "Je Agenten-Tab"): a page target holzi
//! created and attached to in flat mode. Every document it loads, the page itself, a redirect and
//! each frame alike, is paused through `Fetch` until the website check of research R9 has decided,
//! and so is every request that may change something on a website (research R5 step 2), until the
//! user allowed it ([`Guard`]); frames in their own process attach, wait for holzi and get the same
//! checks.
//!
//! Reading happens in holzi's own world ([`super::read`]); the element numbers are those of the
//! reading script and hold until the document changes. The same world shows the marker
//! ([`super::marker`]) while the agent steers the tab; [`Tab::release`] gives the tab back to the
//! user, open and unchecked.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures::future::BoxFuture;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

use super::cdp::{fetch, page, runtime, target, Connection, Event};
use super::error::{BrowserError, Result};
use super::marker::{self, MarkerTexts};
use super::sites::Verdict;
use super::submit;

/// How long a page may take to load before a tool gives up (FR-030).
pub const LOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a page has after a click or an Enter to begin loading a new document.
const SETTLE: Duration = Duration::from_secs(1);

/// How long each step of giving a tab back may take; a busy page must not hold up a stop.
const RELEASE_STEP: Duration = Duration::from_secs(1);

/// A request the tab holds until holzi has decided about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paused {
    pub url: String,
    pub method: String,
    /// A document: a page or a frame.
    pub document: bool,
    /// The body, when the browser hands it over.
    pub body: Option<String>,
}

/// Decides about a held request; may ask the user, so it may take minutes.
pub type Guard = Arc<dyn Fn(Paused) -> BoxFuture<'static, Verdict> + Send + Sync>;

/// What the user's "Stoppen" in the marker does.
pub type OnStop = Arc<dyn Fn() + Send + Sync>;

/// The marker of a tab: its texts and what its stop button does.
pub struct TabMarker {
    pub texts: MarkerTexts,
    pub on_stop: OnStop,
}

/// What the sessions bar shows of a tab: the website and title of the page the agent last saw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageInfo {
    pub site: String,
    pub title: String,
}

/// One agent tab.
pub struct Tab {
    /// `t1`, `t2`, … per session; never an id of the browser.
    pub id: String,
    pub target: String,
    pub session: String,
    connection: Arc<Connection>,
    /// The last document the guard refused in this tab, as the code an agent sees.
    refused: Arc<Mutex<Option<&'static str>>>,
    page: Mutex<PageInfo>,
    guard_task: JoinHandle<()>,
}

impl Tab {
    /// Creates a tab on `about:blank` whose documents `guard` checks and which shows `marker`.
    pub async fn open(
        connection: &Arc<Connection>,
        id: String,
        guard: Guard,
        marker: TabMarker,
    ) -> Result<Self> {
        let target = target::create(connection).await?;
        let session = target::attach(connection, &target).await?;
        let events = connection.subscribe(Some(&session));
        let refused = Arc::new(Mutex::new(None));
        let guard_task = tokio::spawn(watch(
            Arc::clone(connection),
            session.clone(),
            Some(target.clone()),
            events,
            guard,
            Arc::clone(&refused),
            marker.on_stop,
        ));
        let setup = async {
            page::enable(connection, &session).await?;
            marker::install(connection, &session, &marker.texts).await?;
            fetch::hold_requests(connection, &session).await?;
            target::attach_frames(connection, &session).await
        }
        .await;
        if let Err(error) = setup {
            guard_task.abort();
            let _ = target::close(connection, &target).await;
            return Err(error);
        }
        Ok(Self {
            id,
            target,
            session,
            connection: Arc::clone(connection),
            refused,
            page: Mutex::default(),
            guard_task,
        })
    }

    /// The page the agent last saw in this tab.
    pub fn page(&self) -> PageInfo {
        self.page
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set_page(&self, page: PageInfo) {
        *self.page.lock().unwrap_or_else(PoisonError::into_inner) = page;
    }

    /// The events of the tab from now on, taken before an action so nothing it sets off is missed.
    pub fn events(&self) -> UnboundedReceiver<Event> {
        self.connection.subscribe(Some(&self.session))
    }

    /// Waits for what an action set off: a new document of the tab is waited for until it has
    /// loaded; otherwise the page gets [`SETTLE`]. A request refused meanwhile ends with its code.
    /// While `busy` (a question about what the page sends is open), loading has no time limit.
    pub async fn settle(
        &self,
        mut events: UnboundedReceiver<Event>,
        busy: &(dyn Fn() -> bool + Send + Sync),
    ) -> std::result::Result<(), TabError> {
        let main = self.target.as_str();
        let loading = tokio::time::timeout(SETTLE, async {
            while let Some(event) = events.recv().await {
                if event.method == "Page.frameStartedLoading" && event.params["frameId"] == main {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false);
        let mut deadline = tokio::time::Instant::now() + LOAD_TIMEOUT;
        while loading && !self.refusal_pending() {
            match tokio::time::timeout(Duration::from_millis(200), events.recv()).await {
                Ok(Some(event)) if event.method == "Page.loadEventFired" => break,
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(TabError::Browser(BrowserError::Unavailable(
                        "the browser ended".to_owned(),
                    )))
                }
                Err(_) if busy() => deadline = tokio::time::Instant::now() + LOAD_TIMEOUT,
                Err(_) if tokio::time::Instant::now() >= deadline => return Err(TabError::Timeout),
                Err(_) => {}
            }
        }
        match self.take_refusal() {
            Some(code) => Err(TabError::Refused(code)),
            None => Ok(()),
        }
    }

    fn refusal_pending(&self) -> bool {
        self.refused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// Takes the code of the last refused document, if any.
    pub fn take_refusal(&self) -> Option<&'static str> {
        self.refused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Loads `url` and waits until it has loaded. A document the guard refused ends with its code.
    pub async fn go(&self, url: &str) -> std::result::Result<(), TabError> {
        self.take_refusal();
        let mut events = self.connection.subscribe(Some(&self.session));
        let has_loader = match page::navigate(&self.connection, &self.session, url).await {
            Ok(has_loader) => has_loader,
            Err(error) => return Err(self.refusal_or(error)),
        };
        if has_loader {
            self.wait_for_load(&mut events).await
        } else if let Some(code) = self.take_refusal() {
            Err(TabError::Refused(code))
        } else {
            Ok(())
        }
    }

    /// Whether the main document has finished loading.
    pub async fn ready(&self) -> Result<bool> {
        let (frame, _) = page::main_frame(&self.connection, &self.session).await?;
        let context = page::isolated_world(&self.connection, &self.session, &frame).await?;
        let ready = runtime::evaluate(
            &self.connection,
            &self.session,
            context,
            "document.readyState === 'complete'",
        )
        .await?;
        Ok(ready.as_bool().unwrap_or(false))
    }

    /// Goes back or forward in the tab's history; nothing there is no error.
    pub async fn step(&self, back: bool) -> std::result::Result<(), TabError> {
        let (current, entries) = page::history(&self.connection, &self.session).await?;
        let next = if back {
            current.checked_sub(1)
        } else {
            Some(current + 1)
        };
        let Some(&entry) = next.and_then(|index| entries.get(index)) else {
            return Ok(());
        };
        self.take_refusal();
        let mut events = self.connection.subscribe(Some(&self.session));
        page::go_to_entry(&self.connection, &self.session, entry).await?;
        self.wait_for_load(&mut events).await
    }

    /// Loads the page again.
    pub async fn reload(&self) -> std::result::Result<(), TabError> {
        self.take_refusal();
        let mut events = self.connection.subscribe(Some(&self.session));
        page::reload(&self.connection, &self.session).await?;
        self.wait_for_load(&mut events).await
    }

    async fn wait_for_load(
        &self,
        events: &mut UnboundedReceiver<Event>,
    ) -> std::result::Result<(), TabError> {
        let loaded = tokio::time::timeout(LOAD_TIMEOUT, async {
            while let Some(event) = events.recv().await {
                if event.method == "Page.loadEventFired" {
                    return true;
                }
            }
            false
        })
        .await;
        if let Some(code) = self.take_refusal() {
            return Err(TabError::Refused(code));
        }
        match loaded {
            Ok(true) => Ok(()),
            Ok(false) => Err(TabError::Browser(BrowserError::Unavailable(
                "the browser ended".to_owned(),
            ))),
            Err(_) => Err(TabError::Timeout),
        }
    }

    fn refusal_or(&self, error: BrowserError) -> TabError {
        match self.take_refusal() {
            Some(code) => TabError::Refused(code),
            None => TabError::Browser(error),
        }
    }

    /// Scrolls by most of a screen, or brings element `n` into view (`element_gone` when there is
    /// no such element any more).
    pub async fn scroll(&self, down: bool, element: Option<u32>) -> Result<()> {
        let args = match element {
            Some(n) => serde_json::json!({ "scrollTo": n }),
            None => serde_json::json!({ "scrollBy": if down { 1 } else { -1 } }),
        };
        let reply = super::read::script(&self.connection, &self.session, &args).await?;
        if reply.get("gone") == Some(&Value::Bool(true)) {
            return Err(BrowserError::Protocol("element_gone".to_owned()));
        }
        Ok(())
    }

    /// The text of the page now, for waiting on it.
    pub async fn text(&self) -> Result<String> {
        let raw = super::read::raw(&self.connection, &self.session, None).await?;
        Ok(raw.text)
    }

    /// Closes the tab in the browser.
    pub async fn close(self) {
        self.guard_task.abort();
        let _ = target::close(&self.connection, &self.target).await;
    }

    /// Gives the tab back to the user (FR-016): what the agent was loading stops, the tab stays
    /// open, the marker leaves it, and with holzi's session of it go the website check and the
    /// binding.
    pub async fn release(self) {
        self.guard_task.abort();
        let (connection, session) = (&self.connection, self.session.as_str());
        let steps = [
            (
                "stopping the page",
                tokio::time::timeout(RELEASE_STEP, page::stop_loading(connection, session)).await,
            ),
            (
                "removing the marker",
                tokio::time::timeout(RELEASE_STEP, marker::remove(connection, session)).await,
            ),
            (
                "leaving the tab",
                tokio::time::timeout(RELEASE_STEP, target::detach(connection, session)).await,
            ),
        ];
        for (step, outcome) in steps {
            match outcome {
                Ok(Ok(())) => {}
                Ok(Err(error)) => log::debug!("agent browser: {step}: {error}"),
                Err(_) => log::debug!("agent browser: {step}: no answer"),
            }
        }
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        self.guard_task.abort();
    }
}

/// Why loading a page failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabError {
    /// The website check refused the document (`site_not_allowed`, `blocked_address`).
    Refused(&'static str),
    /// Not loaded within [`LOAD_TIMEOUT`].
    Timeout,
    Browser(BrowserError),
}

impl From<BrowserError> for TabError {
    fn from(error: BrowserError) -> Self {
        Self::Browser(error)
    }
}

/// Answers the paused requests of `session` and sets up the frames that attach to it. A refused
/// document of `main` (the tab's own frame, whose id is the target's) and any refused request that
/// would have changed something fail a tool; a refused frame stays empty. A stop from the marker
/// goes to `on_stop`. Boxed, since a frame's own frames are watched by the same function.
fn watch(
    connection: Arc<Connection>,
    session: String,
    main: Option<String>,
    mut events: UnboundedReceiver<Event>,
    guard: Guard,
    refused: Arc<Mutex<Option<&'static str>>>,
    on_stop: OnStop,
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
                    let child_events = connection.subscribe(Some(&child));
                    tokio::spawn(watch(
                        Arc::clone(&connection),
                        child.clone(),
                        None,
                        child_events,
                        Arc::clone(&guard),
                        Arc::clone(&refused),
                        Arc::clone(&on_stop),
                    ));
                    let connection = Arc::clone(&connection);
                    tokio::spawn(async move {
                        // The frame waits for holzi; its documents are held before it may go on.
                        let _ = fetch::hold_requests(&connection, &child).await;
                        let _ = target::attach_frames(&connection, &child).await;
                        let _ = runtime::run_if_waiting(&connection, &child).await;
                    });
                }
                "Runtime.bindingCalled"
                    if event.params["name"] == marker::BINDING
                        && event.params["payload"] == marker::STOP =>
                {
                    on_stop();
                }
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

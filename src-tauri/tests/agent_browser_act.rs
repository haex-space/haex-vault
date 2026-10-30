//! Spec 065 T055 against a real Chromium and the fixture site: while an agent steers a tab, the
//! tab carries the marker with the agent's name in a closed shadow root and a prefix on its title;
//! the page cannot reach the marker's binding, and its synthetic events do nothing, while the
//! user's real click on "Stoppen" stops the session and gives the tab back. A stop ends a call
//! that waits for a slow page within a second (SC-004), and further calls are refused until a new
//! session begins. Without a Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use agent_browser_scene::{agent, content, scene, Scene};
use holzi_lib::agent_browser::cdp::Connection;
use holzi_lib::agent_browser::error::BrowserError;

/// A session of the test's own on the agent's tab, in the page's world: what the page sees.
async fn page_session(scene: &Scene) -> (std::sync::Arc<Connection>, String) {
    let connection = scene
        .browser
        .ensure(|| Err(BrowserError::Unavailable("already running".to_owned())))
        .await
        .unwrap();
    let targets = connection
        .call("Target.getTargets", json!({}), None)
        .await
        .unwrap();
    let target = targets["targetInfos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| {
            t["type"] == "page" && t["url"].as_str().unwrap_or_default().contains("127.0.0.1")
        })
        .expect("the agent's tab")["targetId"]
        .as_str()
        .unwrap()
        .to_owned();
    let session = connection
        .call(
            "Target.attachToTarget",
            json!({ "targetId": target, "flatten": true }),
            None,
        )
        .await
        .unwrap()["sessionId"]
        .as_str()
        .unwrap()
        .to_owned();
    (connection, session)
}

/// Runs `expression` in the page's own world.
async fn in_page(connection: &Connection, session: &str, expression: &str) -> Value {
    connection
        .call(
            "Runtime.evaluate",
            json!({ "expression": expression, "returnByValue": true, "awaitPromise": true }),
            Some(session),
        )
        .await
        .unwrap()["result"]["value"]
        .clone()
}

/// Waits up to five seconds for `done`.
async fn eventually(what: &str, mut done: impl AsyncFnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if done().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{what} did not happen within five seconds");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_user_sees_the_agent_and_stops_it() {
    let Some(scene) = scene().await else {
        return;
    };

    // The marker: in the tab, with the agent's name, its title prefixed; the agent reads the
    // page's own title.
    let page = scene
        .auto("browser.open", json!({ "url": scene.site.url("/") }))
        .await
        .expect("opened");
    assert_eq!(content(&page)["title"], "Nachrichten");
    let sessions = scene.sessions.list();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].agent, "Testagent");
    assert_eq!(sessions[0].tabs[0].tab, "t1");
    assert_eq!(sessions[0].tabs[0].title, "Nachrichten");
    assert_eq!(
        sessions[0].last_action.as_ref().unwrap().tool,
        "browser.open"
    );
    let (connection, session) = page_session(&scene).await;
    let seen = in_page(
        &connection,
        &session,
        "({ marker: !!document.querySelector('holzi-agent-marker'), \
            open: document.querySelector('holzi-agent-marker')?.shadowRoot ?? null, \
            binding: typeof window.__holzi, title: document.title })",
    )
    .await;
    assert_eq!(seen["marker"], true, "{seen}");
    assert_eq!(seen["open"], Value::Null, "the shadow root is closed");
    assert_eq!(seen["binding"], "undefined", "the page has no binding");
    assert_eq!(seen["title"], "[Agent] Nachrichten");

    // The marker is in the next document too, and survives the page removing it.
    scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/article.html") }),
        )
        .await
        .expect("opened");
    let back = in_page(
        &connection,
        &session,
        "(async () => { document.querySelector('holzi-agent-marker').remove(); \
            await new Promise((r) => setTimeout(r, 100)); \
            return !!document.querySelector('holzi-agent-marker') })()",
    )
    .await;
    assert_eq!(back, true, "the marker came back");

    // What the page can do does not stop anything: synthetic clicks and events on the marker.
    in_page(
        &connection,
        &session,
        "(() => { const host = document.querySelector('holzi-agent-marker'); host.click(); \
            host.dispatchEvent(new MouseEvent('click', { bubbles: true, composed: true })); })()",
    )
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(scene.sessions.list().len(), 1, "still steering");

    // The user's real click on "Stoppen" (the right end of the bar) stops the session.
    let rect = in_page(
        &connection,
        &session,
        "(() => { const r = document.querySelector('holzi-agent-marker').getBoundingClientRect(); \
            return { x: r.right - 30, y: r.top + r.height / 2 } })()",
    )
    .await;
    for kind in ["mousePressed", "mouseReleased"] {
        connection
            .call(
                "Input.dispatchMouseEvent",
                json!({ "type": kind, "x": rect["x"], "y": rect["y"], "button": "left", "clickCount": 1 }),
                Some(&session),
            )
            .await
            .unwrap();
    }
    eventually("the stop", async || scene.sessions.list().is_empty()).await;
    eventually("the marker to go", async || {
        in_page(
            &connection,
            &session,
            "!document.querySelector('holzi-agent-marker') && !document.title.startsWith('[Agent]')",
        )
        .await
            == true
    })
    .await;
    let refused = scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .expect_err("stopped");
    assert_eq!(refused.code, "stopped");
    let url = in_page(&connection, &session, "location.href").await;
    assert!(
        url.as_str().unwrap().contains("article.html"),
        "the tab stays open for the user: {url}"
    );

    // A new session; a stop ends a call that waits for a slow page within a second.
    scene.sessions.resume(&agent());
    scene
        .auto("browser.open", json!({ "url": scene.site.url("/") }))
        .await
        .expect("a new session");
    let waiting = {
        let tools = &scene;
        async move {
            tools
                .auto("browser.open", json!({ "url": tools.site.url("/hang") }))
                .await
        }
    };
    let stopping = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let id = scene.sessions.list()[0].id.clone();
        let asked = Instant::now();
        assert!(scene.sessions.stop(&id).await);
        asked
    };
    let (outcome, asked) = tokio::join!(waiting, stopping);
    let ended = Instant::now();
    assert_eq!(outcome.expect_err("stopped").code, "stopped");
    assert!(
        ended.duration_since(asked) < Duration::from_secs(1),
        "within a second (SC-004): {:?}",
        ended.duration_since(asked)
    );

    // Revoking ends a session, stopped or not; the next call begins anew.
    scene.sessions.end(&agent()).await;
    scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .expect_err("a new session has no tab t1");

    scene.browser.stop().await.unwrap();
}

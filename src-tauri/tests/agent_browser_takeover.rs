//! Spec 065 FR-017, FR-018 against a real Chromium and the fixture site over HTTPS: the agent's own
//! clicks and typing do not pause it, while the user's click in the page does; "Eingreifen" and
//! "Zurückgeben" in the marker switch who has the browser. While the user has it, the agent's calls
//! get `paused`, a running call ends within a second, and the tab loads what the user opens
//! without a question; the new page's marker offers "Zurückgeben" too. Giving back asks about a
//! page outside the agent's rights; declined, the tab stays with the user.
//! `browser.takeover.request` asks with the agent's reason and waits until the user gives back.
//! Without a Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use agent_browser_scene::{content, scene_with, Scene};
use holzi_lib::agent_browser::cdp::Connection;
use holzi_lib::agent_browser::error::BrowserError;
use holzi_lib::agent_browser::prompts::BrowserPromptKind;
use holzi_lib::agent_browser::sessions::{BrowserControl, BrowserPause};
use holzi_lib::chat::tools::native_action::NativeReply;
use holzi_lib::mcp_server::grants::SiteLevel;

/// The number of the element named `name` on the page of `reply`.
fn element(reply: &NativeReply, name: &str) -> Value {
    content(reply)["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["name"] == name)
        .unwrap_or_else(|| panic!("no element {name}: {}", reply.result))["n"]
        .clone()
}

/// A session of the test's own on the tab showing `host`: what the user and the page do.
async fn user_session(scene: &Scene, host: &str) -> (Arc<Connection>, String) {
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
        .find(|t| t["type"] == "page" && t["url"].as_str().unwrap_or_default().contains(host))
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

/// A real click of the user at `(x, y)`.
async fn click(connection: &Connection, session: &str, x: f64, y: f64) {
    for kind in ["mousePressed", "mouseReleased"] {
        connection
            .call(
                "Input.dispatchMouseEvent",
                json!({ "type": kind, "x": x, "y": y, "button": "left", "clickCount": 1 }),
                Some(session),
            )
            .await
            .unwrap();
    }
}

/// Clicks along the marker from its left end until `done`: the first button there is
/// "Eingreifen" or "Zurückgeben" (its shadow root is closed, so its buttons cannot be looked up).
async fn steer_in_marker(connection: &Connection, session: &str, done: impl Fn() -> bool) {
    let bar = in_page(
        connection,
        session,
        "(() => { const r = document.querySelector('holzi-agent-marker').getBoundingClientRect(); \
            return { left: r.left, right: r.right, y: r.top + r.height / 2 } })()",
    )
    .await;
    let (left, right, y) = (
        bar["left"].as_f64().unwrap(),
        bar["right"].as_f64().unwrap(),
        bar["y"].as_f64().unwrap(),
    );
    let mut x = left + 8.0;
    // The stop button is at the right end; stay clear of it.
    while x < right - 90.0 {
        click(connection, session, x, y).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        if done() {
            return;
        }
        x += 12.0;
    }
    panic!("no steering button in the marker");
}

fn pause(scene: &Scene) -> Option<BrowserPause> {
    scene.sessions.list().first().and_then(|s| s.paused.clone())
}

/// Waits up to five seconds for `done`.
async fn eventually(what: &str, done: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{what} did not happen within five seconds");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_user_takes_over_and_gives_back() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };

    // The agent's own typing and clicking do not pause it.
    let page = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/form.html") }),
        )
        .await
        .expect("opened");
    let name = element(&page, "Name");
    scene
        .auto(
            "browser.type",
            json!({ "tab": "t1", "element": name, "text": "Ana" }),
        )
        .await
        .expect("typed");
    let details = element(&page, "Details zeigen");
    scene
        .auto("browser.click", json!({ "tab": "t1", "element": details }))
        .await
        .expect("clicked");
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(pause(&scene), None, "the agent's own input");

    // The user's click in the page pauses the agent.
    let (connection, user) = user_session(&scene, "127.0.0.1").await;
    let field = in_page(
        &connection,
        &user,
        "(() => { const r = document.getElementById('name').getBoundingClientRect(); \
            return { x: r.left + 5, y: r.top + r.height / 2 } })()",
    )
    .await;
    click(
        &connection,
        &user,
        field["x"].as_f64().unwrap(),
        field["y"].as_f64().unwrap(),
    )
    .await;
    eventually("the pause", || pause(&scene) == Some(BrowserPause::User)).await;
    let refused = scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .expect_err("paused");
    assert_eq!(refused.code, "paused");

    // "Zurückgeben" in the marker, then "Eingreifen" there.
    steer_in_marker(&connection, &user, || pause(&scene).is_none()).await;
    scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .expect("the agent steers again");
    steer_in_marker(&connection, &user, || pause(&scene).is_some()).await;

    // What the user opens loads without a question, even outside the agent's rights.
    let partner = scene.site.partner("/partner.html");
    connection
        .call("Page.navigate", json!({ "url": partner }), Some(&user))
        .await
        .unwrap();
    eventually("the partner page", || {
        scene
            .site
            .seen()
            .iter()
            .any(|seen| seen.path == "/partner.html")
    })
    .await;
    assert!(scene.asked.lock().unwrap().is_empty(), "nothing asked");

    // The new page's marker offers "Zurückgeben" too. Giving back there asks about the page;
    // declined, the tab stays with the user.
    let id = scene.sessions.list()[0].id.clone();
    let until = Instant::now() + Duration::from_secs(5);
    while in_page(
        &connection,
        &user,
        "location.host.startsWith('localhost') && !!document.querySelector('holzi-agent-marker')",
    )
    .await
        != true
    {
        assert!(Instant::now() < until, "the partner page with its marker");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    steer_in_marker(&connection, &user, || pause(&scene).is_none()).await;
    let asked = scene.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].kind, BrowserPromptKind::Site);
    let listed = scene.auto("browser.tabs.list", json!({})).await.unwrap();
    assert_eq!(
        content(&listed)["tabs"],
        json!([]),
        "the tab left the agent"
    );
    let marked = in_page(
        &connection,
        &user,
        "!!document.querySelector('holzi-agent-marker')",
    )
    .await;
    assert_eq!(marked, false, "without marker");

    // Taking over ends a running call within a second.
    let waiting = scene.auto(
        "browser.tabs.open",
        json!({ "url": scene.site.url("/hang") }),
    );
    let taking = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let asked = Instant::now();
        assert!(scene.sessions.control(&id, BrowserControl::Takeover).await);
        asked
    };
    let (outcome, asked) = tokio::join!(waiting, taking);
    assert_eq!(outcome.expect_err("paused").code, "paused");
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "within a second: {:?}",
        asked.elapsed()
    );
    assert!(scene.sessions.control(&id, BrowserControl::Return).await);
    eventually("the return", || pause(&scene).is_none()).await;

    scene.browser.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_agent_asks_the_user_to_take_over() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    scene
        .auto("browser.open", json!({ "url": scene.site.url("/") }))
        .await
        .expect("opened");

    // Declined: the agent goes on.
    let declined = scene
        .auto(
            "browser.takeover.request",
            json!({ "tab": "t1", "reason": "Bitte das Captcha lösen" }),
        )
        .await
        .expect_err("declined");
    assert_eq!(declined.code, "user_declined");
    assert_eq!(pause(&scene), None);
    let asked = scene.asked.lock().unwrap().clone();
    assert_eq!(asked[0].kind, BrowserPromptKind::Takeover);
    assert_eq!(asked[0].reason.as_deref(), Some("Bitte das Captcha lösen"));

    // Allowed: the call waits until the user gives back.
    *scene.allow.lock().unwrap() = true;
    let requesting = scene.auto(
        "browser.takeover.request",
        json!({ "tab": "t1", "reason": "Zwei-Faktor-Code" }),
    );
    let user = async {
        eventually("the pause", || pause(&scene).is_some()).await;
        assert_eq!(
            pause(&scene),
            Some(BrowserPause::Request {
                reason: "Zwei-Faktor-Code".to_owned()
            })
        );
        let refused = scene
            .auto("browser.page.read", json!({ "tab": "t1" }))
            .await
            .expect_err("paused");
        assert_eq!(refused.code, "paused");
        tokio::time::sleep(Duration::from_millis(300)).await;
        let id = scene.sessions.list()[0].id.clone();
        assert!(scene.sessions.control(&id, BrowserControl::Return).await);
    };
    let (returned, ()) = tokio::join!(requesting, user);
    assert_eq!(returned.expect("returned").result["returned"], true);
    scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .expect("the agent steers again");

    let long = "x".repeat(201);
    let refused = scene
        .auto(
            "browser.takeover.request",
            json!({ "tab": "t1", "reason": long }),
        )
        .await
        .expect_err("too long");
    assert_eq!(refused.code, "invalid_input");

    scene.browser.stop().await.unwrap();
}

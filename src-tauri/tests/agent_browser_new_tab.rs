//! Spec 065 T053 `newTab` against a real Chromium and the fixture site over HTTPS: a tab a page of
//! an agent tab opens, through `window.open` or a link with `target=_blank` (which Chromium opens
//! without an opener), loads nothing before holzi decided. In Auto it asks for another website: a
//! declined tab loads nothing and closes, an allowed one joins the agent's tabs with the marker. A
//! tab on an allowed website joins without a question, and at five tabs a new one closes at once.
//! Without a Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use agent_browser_scene::{content, scene_with, Scene};
use holzi_lib::agent_browser::error::BrowserError;
use holzi_lib::agent_browser::prompts::BrowserPromptKind;
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

fn asked(scene: &Scene) -> Vec<(BrowserPromptKind, String)> {
    scene
        .asked
        .lock()
        .unwrap()
        .iter()
        .map(|q| (q.kind, q.label.clone().unwrap_or_default()))
        .collect()
}

/// The agent's tabs: id, website and title.
async fn tabs(scene: &Scene) -> Vec<Value> {
    let listed = scene.auto("browser.tabs.list", json!({})).await.unwrap();
    content(&listed)["tabs"].as_array().unwrap().clone()
}

/// Waits up to five seconds until the agent has `count` tabs.
async fn tab_count(scene: &Scene, count: usize) -> Vec<Value> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let listed = tabs(scene).await;
        if listed.len() == count || Instant::now() >= until {
            assert_eq!(listed.len(), count, "{listed:?}");
            return listed;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// What the page of the browser's tab at an address containing `part` sees of the marker.
async fn marker_of(scene: &Scene, part: &str) -> Value {
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
        .find(|t| t["type"] == "page" && t["url"].as_str().unwrap_or_default().contains(part))
        .unwrap_or_else(|| panic!("no tab at {part}: {targets}"))["targetId"]
        .clone();
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
    connection
        .call(
            "Runtime.evaluate",
            json!({
                "expression": "({ marker: !!document.querySelector('holzi-agent-marker'), \
                    title: document.title })",
                "returnByValue": true,
            }),
            Some(&session),
        )
        .await
        .unwrap()["result"]["value"]
        .clone()
}

fn partner_requests(scene: &Scene) -> Vec<String> {
    scene
        .site
        .seen()
        .into_iter()
        .filter(|seen| seen.host.starts_with("localhost") && seen.path.starts_with("/partner"))
        .map(|seen| seen.path)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_tab_a_page_opens_is_checked_before_it_loads() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let page = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/popup.html") }),
        )
        .await
        .expect("the page");

    // Declined: the other website gets no request, and the tab closes.
    scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": element(&page, "Fenster öffnen") }),
        )
        .await
        .expect("the click returns");
    let partner = scene
        .site
        .url("/partner.html?via=open")
        .replace("127.0.0.1", "localhost");
    assert_eq!(asked(&scene), [(BrowserPromptKind::NewTab, partner)]);
    assert!(partner_requests(&scene).is_empty());
    tab_count(&scene, 1).await;

    // A link with `target=_blank` has no opener, yet is held just the same.
    *scene.allow.lock().unwrap() = true;
    scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": element(&page, "Partner im neuen Tab") }),
        )
        .await
        .expect("the click returns");
    let partner = scene
        .site
        .url("/partner.html?via=link")
        .replace("127.0.0.1", "localhost");
    assert_eq!(asked(&scene)[1], (BrowserPromptKind::NewTab, partner));
    let listed = tab_count(&scene, 2).await;
    let joined = listed
        .iter()
        .find(|tab| tab["title"] == "Partner")
        .unwrap_or_else(|| panic!("the new tab: {listed:?}"));
    assert_eq!(partner_requests(&scene), ["/partner.html?via=link"]);
    let read = scene
        .auto("browser.page.read", json!({ "tab": joined["tab"] }))
        .await
        .expect("the agent reads the new tab");
    assert!(content(&read)["text"]
        .as_str()
        .unwrap()
        .contains("Partnerseite"));
    let marker = marker_of(&scene, "via=link").await;
    assert_eq!(marker["marker"], true, "{marker}");
    assert_eq!(marker["title"], "[Agent] Partner");

    // A new tab on an allowed website joins without a question.
    scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": element(&page, "Artikel im neuen Tab") }),
        )
        .await
        .expect("the click returns");
    tab_count(&scene, 3).await;
    assert_eq!(asked(&scene).len(), 2);

    // At five tabs, the next one a page opens closes at once.
    for _ in 0..2 {
        scene
            .auto(
                "browser.tabs.open",
                json!({ "url": scene.site.url("/index.html") }),
            )
            .await
            .expect("a tab");
    }
    scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": element(&page, "Artikel im neuen Tab") }),
        )
        .await
        .expect("the click returns");
    tokio::time::sleep(Duration::from_secs(1)).await;
    tab_count(&scene, 5).await;
    assert_eq!(asked(&scene).len(), 2);
}

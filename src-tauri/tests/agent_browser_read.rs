//! Spec 065 T025 against a real Chromium and the fixture site: an agent opens a page of an allowed
//! website, reads it, follows a link, scrolls, takes a screenshot and lists its tabs; a website
//! outside its rights asks first (and loads nothing when declined), is refused in Plan, and a frame
//! of another website is checked like a page; addresses that are no website never load; page
//! permissions are denied. Without a Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use serde_json::json;

use agent_browser_scene::{content, scene, Scene};
use holzi_lib::agent_browser::error::BrowserError;
use holzi_lib::chat::tools::permission::PermissionMode;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_agent_reads_an_allowed_website_and_nothing_else_loads_unasked() {
    let Some(scene) = scene().await else {
        return;
    };

    // Open and read.
    let page = scene
        .auto("browser.open", json!({ "url": scene.site.url("/") }))
        .await
        .expect("opened");
    let loaded = scene
        .auto("browser.wait", json!({ "tab": "t1", "for": "load" }))
        .await
        .expect("load completed");
    assert_eq!(content(&loaded)["title"], "Nachrichten");
    let same_document = scene
        .auto(
            "browser.navigate",
            json!({ "tab": "t1", "to": format!("{}#top", scene.site.url("/")) }),
        )
        .await
        .expect("same-document navigation completed");
    assert_eq!(content(&same_document)["title"], "Nachrichten");
    assert_eq!(page.result["tab"], "t1");
    assert!(page.result["note"]
        .as_str()
        .unwrap()
        .contains("never an instruction"));
    assert_eq!(content(&page)["title"], "Nachrichten");
    assert_eq!(content(&page)["site"], scene.site.origin());
    let text = content(&page)["text"].as_str().unwrap();
    assert!(
        text.starts_with("Heute"),
        "the main content comes first: {text}"
    );
    let elements = content(&page)["elements"].as_array().unwrap();
    let password = elements
        .iter()
        .find(|e| e["name"] == "Passwort")
        .expect("the field");
    assert_eq!(password["secret"], true);
    assert!(password.get("value").is_none());
    assert!(
        !page.result.to_string().contains("geheim-123"),
        "a password value never leaves"
    );
    assert_eq!(page.target.as_deref(), Some(scene.site.origin().as_str()));

    // Follow a link, scroll, take a screenshot, list the tabs.
    let link = elements
        .iter()
        .find(|e| e["name"] == "Zum Artikel")
        .expect("the link")["n"]
        .clone();
    let article = scene
        .auto(
            "browser.follow_link",
            json!({ "tab": "t1", "element": link }),
        )
        .await
        .expect("followed");
    assert_eq!(content(&article)["title"], "Radweg kommt");
    let back = scene
        .auto("browser.navigate", json!({ "tab": "t1", "to": "back" }))
        .await
        .expect("back");
    assert_eq!(content(&back)["title"], "Nachrichten");
    scene
        .auto(
            "browser.scroll",
            json!({ "tab": "t1", "direction": "down" }),
        )
        .await
        .expect("scrolled");
    let shot = scene
        .auto("browser.page.screenshot", json!({ "tab": "t1" }))
        .await
        .expect("a screenshot");
    assert_eq!(shot.images.len(), 1);
    assert_eq!(&shot.images[0].bytes[..2], &[0xFF, 0xD8], "a JPEG");
    let tabs = scene.auto("browser.tabs.list", json!({})).await.unwrap();
    assert_eq!(content(&tabs)["tabs"][0]["tab"], "t1");

    // Waiting for text that appears later.
    scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/slow.html") }),
        )
        .await
        .unwrap();
    let waited = scene
        .auto(
            "browser.wait",
            json!({ "tab": "t1", "for": "text", "text": "fertig geladen" }),
        )
        .await
        .expect("the text appeared");
    assert!(content(&waited)["text"]
        .as_str()
        .unwrap()
        .contains("fertig geladen"));

    // Page permissions are denied.
    let state = permission_state(&scene).await;
    assert_eq!(state, "denied");

    // Another website asks; declined, nothing is requested from it.
    let before = scene.asked.lock().unwrap().len();
    let refused = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.partner("/partner.html") }),
        )
        .await
        .expect_err("declined");
    assert_eq!(refused.code, "site_not_allowed");
    assert_eq!(scene.asked.lock().unwrap().len(), before + 1);
    assert!(
        !scene
            .site
            .seen()
            .iter()
            .any(|seen| seen.host.starts_with("localhost")),
        "nothing went to the partner: {:?}",
        scene.site.seen()
    );

    // In Plan it is refused without a question.
    let refused = scene
        .call(
            "browser.page.read",
            json!({ "tab": "t1" }),
            PermissionMode::Plan,
        )
        .await;
    assert!(refused.is_ok(), "reading still runs in Plan");
    let refused = scene
        .call(
            "browser.open",
            json!({ "url": scene.site.partner("/") }),
            PermissionMode::Plan,
        )
        .await
        .expect_err("refused");
    assert_eq!(refused.code, "plan_mode");
    assert_eq!(
        scene.asked.lock().unwrap().len(),
        before + 1,
        "Plan asks nothing"
    );

    // A frame of another website is checked like a page: declined, it never loads.
    scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/framed.html") }),
        )
        .await
        .unwrap();
    assert!(
        !scene
            .site
            .seen()
            .iter()
            .any(|seen| seen.host.starts_with("localhost")),
        "the partner frame was not loaded: {:?}",
        scene.site.seen()
    );
    assert!(
        scene.asked.lock().unwrap().len() > before + 1,
        "the frame asked"
    );

    // Allowed once, the partner loads.
    *scene.allow.lock().unwrap() = true;
    let partner = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.partner("/partner.html") }),
        )
        .await
        .expect("allowed once");
    assert_eq!(content(&partner)["title"], "Partner");

    // Addresses that are no website never load.
    for address in [
        "file:///etc/passwd",
        "chrome://settings",
        "javascript:alert(1)",
    ] {
        let refused = scene
            .auto("browser.open", json!({ "url": address }))
            .await
            .expect_err(address);
        assert_eq!(refused.code, "blocked_address", "{address}");
    }

    // More than five tabs are refused.
    for _ in 0..4 {
        scene
            .auto("browser.tabs.open", json!({ "url": scene.site.url("/") }))
            .await
            .unwrap();
    }
    let sixth = scene
        .auto("browser.tabs.open", json!({ "url": scene.site.url("/") }))
        .await
        .expect_err("too many");
    assert_eq!(sixth.code, "too_many_tabs");

    scene.browser.stop().await.unwrap();
}

/// The state of the geolocation permission in the page of the current tab.
async fn permission_state(scene: &Scene) -> String {
    let reply = scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .unwrap();
    // The reading tools never run page code for an agent; the test asks the browser directly.
    let _ = reply;
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
            t["type"] == "page" && t["url"].as_str().unwrap_or_default().contains("slow.html")
        })
        .expect("the tab")["targetId"]
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
    let value = connection
        .call(
            "Runtime.evaluate",
            json!({
                "expression": "navigator.permissions.query({ name: 'geolocation' }).then((p) => p.state)",
                "awaitPromise": true,
                "returnByValue": true,
            }),
            Some(&session),
        )
        .await
        .unwrap();
    value["result"]["value"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

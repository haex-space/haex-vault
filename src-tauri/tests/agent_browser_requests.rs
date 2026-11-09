//! Spec 065 FR-044 against a real Chromium and the fixture site over HTTPS: a request a page sends
//! on its own asks with its own kind; "Für diese Website merken" turns the answer into a rule for
//! the website the request goes to, and the next such request follows the rule without asking.
//! A reading request of a page never reaches a website the user blocked (FR-047).
//! Without a Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use agent_browser_scene::{content, scene_with, Scene};
use holzi_lib::agent_browser::prompts::BrowserPromptKind;
use holzi_lib::agent_browser::sites::Site;
use holzi_lib::chat::tools::native_action::NativeReply;
use holzi_lib::mcp_server::grants::{RequestRule, SiteGrant, SiteLevel};

fn element(reply: &NativeReply, name: &str) -> Value {
    content(reply)["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["name"] == name)
        .unwrap_or_else(|| panic!("no element {name}: {}", reply.result))
        .clone()
}

fn request_questions(scene: &Scene) -> usize {
    scene
        .asked
        .lock()
        .unwrap()
        .iter()
        .filter(|q| q.kind == BrowserPromptKind::Request)
        .count()
}

/// Opens the form and clicks "Merken": the page then sends `POST /api` on its own.
async fn page_sends_a_request(scene: &Scene) {
    let page = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/form.html") }),
        )
        .await
        .expect("the form");
    scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": element(&page, "Merken")["n"] }),
        )
        .await
        .expect("clicked");
}

async fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_remembered_answer_becomes_a_rule_for_the_website_the_request_goes_to() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let site = Site::parse(&scene.site.url("/")).expect("site");

    // Declined and remembered: the question says it is the page's request, and a rule is stored.
    *scene.remember.lock().unwrap() = Some("request");
    page_sends_a_request(&scene).await;
    wait_until("the page request question", || {
        request_questions(&scene) == 1
    })
    .await;
    let question = scene
        .asked
        .lock()
        .unwrap()
        .iter()
        .find(|q| q.kind == BrowserPromptKind::Request)
        .cloned()
        .unwrap();
    assert!(question.label.as_deref().unwrap().ends_with("/api"));
    assert_eq!(question.site.as_deref(), Some(site.as_str()));
    wait_until("the rule", || {
        scene.env.rights.lock().unwrap().requests.get(&site) == Some(&RequestRule::Deny)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(scene.site.count("POST", "/api"), 0, "declined, never sent");

    // The next one is refused by the rule, without a question.
    page_sends_a_request(&scene).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(request_questions(&scene), 1, "the rule answered");
    assert_eq!(scene.site.count("POST", "/api"), 0);

    // An allow rule sends without asking.
    scene
        .env
        .rights
        .lock()
        .unwrap()
        .requests
        .insert(site.clone(), RequestRule::Allow);
    page_sends_a_request(&scene).await;
    wait_until("the allowed request", || {
        scene.site.count("POST", "/api") == 1
    })
    .await;
    assert_eq!(request_questions(&scene), 1, "still the one question");

    scene.browser.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reading_fetch_never_reaches_a_blocked_website() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let partner = Site::parse(&scene.site.partner("/")).expect("site");
    scene.env.rights.lock().unwrap().sites.insert(
        partner,
        SiteGrant {
            level: SiteLevel::Blocked,
            fill_remembered: false,
        },
    );
    scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/fetches.html") }),
        )
        .await
        .expect("a page that fetches opens; the refused fetch fails only in the page");
    let from = |host: &str| {
        scene
            .site
            .seen()
            .iter()
            .filter(|seen| seen.path == "/invoice.txt" && seen.host.starts_with(host))
            .count()
    };
    wait_until("the page's own fetch", || from("127.0.0.1") == 1).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(from("localhost"), 0, "the blocked website got nothing");
    assert_eq!(request_questions(&scene), 0);
    scene.browser.stop().await.unwrap();
}

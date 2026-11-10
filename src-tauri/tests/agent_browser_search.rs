//! Spec 065 revision 2026-11-02 (FR-039, FR-040) against a real Chromium and the fixture site: a
//! search opens the engine's result page with the encoded query and reads it; a built-in agent whose
//! browser is off is asked once, and allowing switches the browser on, declining refuses. Without a
//! Chromium-based browser the test says so and stops.

#[allow(dead_code)]
#[path = "common/agent_browser_fixture.rs"]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use serde_json::json;

use agent_browser_scene::{content, scene_with};
use holzi_lib::agent_browser::prompts::BrowserPromptKind;
use holzi_lib::caller::Caller;
use holzi_lib::chat::tools::permission::PermissionMode;
use holzi_lib::mcp_server::grants::SiteLevel;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_search_opens_the_result_page_and_an_off_browser_asks_first() {
    let Some(scene) = scene_with(false, SiteLevel::Read).await else {
        return;
    };

    let page = scene
        .auto(
            "browser.search",
            json!({ "query": "Wetter Berlin & Umland" }),
        )
        .await
        .expect("searched");
    assert_eq!(content(&page)["title"], "Nachrichten");
    let url = content(&page)["url"].as_str().unwrap();
    assert!(url.ends_with("/?q=Wetter+Berlin+%26+Umland"), "{url}");
    assert!(
        scene.asked.lock().unwrap().is_empty(),
        "an allowed website asks nothing"
    );

    // The built-in agent's browser is off: in Plan nothing could open a page, so it is not asked.
    *scene.env.off.lock().unwrap() = true;
    let planned = scene
        .call_as(
            Caller::BuiltinAgent,
            "browser.search",
            json!({ "query": "x" }),
            PermissionMode::Plan,
        )
        .await
        .expect_err("refused in Plan");
    assert_eq!(planned.code, "plan_mode");
    assert!(scene.asked.lock().unwrap().is_empty(), "Plan asks nothing");

    // Otherwise it asks; declining refuses and loads nothing.
    let declined = scene
        .call_as(
            Caller::BuiltinAgent,
            "browser.search",
            json!({ "query": "x" }),
            PermissionMode::Auto,
        )
        .await
        .expect_err("declined");
    assert_eq!(declined.code, "user_declined");
    assert!(*scene.env.off.lock().unwrap(), "still off");
    {
        let asked = scene.asked.lock().unwrap();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].kind, BrowserPromptKind::SwitchOn);
        assert_eq!(asked[0].agent, "Testagent");
    }

    // Allowing switches it on and the search runs.
    *scene.allow.lock().unwrap() = true;
    let page = scene
        .call_as(
            Caller::BuiltinAgent,
            "browser.search",
            json!({ "query": "Wetter" }),
            PermissionMode::Auto,
        )
        .await
        .expect("allowed and searched");
    assert_eq!(content(&page)["title"], "Nachrichten");
    assert!(!*scene.env.off.lock().unwrap(), "switched on");

    // An external agent is never asked: its rights come from the MCP settings.
    *scene.env.off.lock().unwrap() = true;
    let before = scene.asked.lock().unwrap().len();
    let refused = scene
        .auto("browser.search", json!({ "query": "x" }))
        .await
        .expect_err("refused");
    assert_eq!(refused.code, "not_allowed");
    assert_eq!(scene.asked.lock().unwrap().len(), before);
}

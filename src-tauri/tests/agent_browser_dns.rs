//! Spec 065 FR-048 against a real Chromium: the DNS over HTTPS choice reaches the browser through
//! the profile. With "strict" and a provider that does not answer, a website does not load; with
//! "automatic" and the same provider the browser falls back to the system's resolver and it does.
//! The checks need a name the system can resolve, so they are skipped without a network (and,
//! as the others, without a Chromium-based browser).

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use std::net::ToSocketAddrs;

use serde_json::json;

use agent_browser_scene::{scene_with, Scene};
use holzi_lib::agent_browser::dns::{DnsMode, DnsProvider, DnsSettings};
use holzi_lib::agent_browser::error::BrowserError;
use holzi_lib::mcp_server::grants::SiteLevel;

/// A provider that nothing listens at: the discard port of this machine.
const DEAD: &str = "https://127.0.0.1:9/dns-query";
/// A name every network can resolve.
const WEBSITE: &str = "https://example.org/";

fn settings(mode: DnsMode) -> DnsSettings {
    DnsSettings {
        mode,
        provider: DnsProvider::Custom,
        custom: DEAD.to_owned(),
    }
}

/// The error text of loading `WEBSITE` in a blank tab; `None` when it loaded.
async fn load_error(scene: &Scene) -> Option<String> {
    // The first tool call starts the browser, with the choice the scene holds.
    scene.auto("browser.tabs.list", json!({})).await.unwrap();
    let connection = scene
        .browser
        .ensure(|| Err(BrowserError::Unavailable("already running".to_owned())))
        .await
        .unwrap();
    let target = connection
        .call("Target.createTarget", json!({ "url": "about:blank" }), None)
        .await
        .unwrap();
    let target = target["targetId"].as_str().unwrap().to_owned();
    let session = connection
        .call(
            "Target.attachToTarget",
            json!({ "targetId": target, "flatten": true }),
            None,
        )
        .await
        .unwrap();
    let session = session["sessionId"].as_str().unwrap().to_owned();
    let reply = connection
        .call("Page.navigate", json!({ "url": WEBSITE }), Some(&session))
        .await
        .unwrap();
    reply["errorText"].as_str().map(str::to_owned)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_choice_reaches_the_browser_strict_fails_and_automatic_falls_back() {
    if ("example.org", 443).to_socket_addrs().is_err() {
        eprintln!("skipped: no network to resolve a name");
        return;
    }
    for (mode, loads) in [(DnsMode::Secure, false), (DnsMode::Automatic, true)] {
        let Some(scene) = scene_with(false, SiteLevel::Read).await else {
            return;
        };
        *scene.env.dns.lock().unwrap() = settings(mode);
        let error = load_error(&scene).await;
        assert_eq!(
            error.is_none(),
            loads,
            "{mode:?} with a provider that does not answer: {error:?}"
        );
        scene.browser.stop().await.unwrap();
    }
}

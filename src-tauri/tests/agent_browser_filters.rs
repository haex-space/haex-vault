//! Spec 065 FR-045 against a real Chromium and the fixture site over HTTPS: requests a filter list
//! blocks never leave the browser and raise no question, a request no list names goes on, a page a
//! list blocks fails with its own code, and with no filter nothing is blocked. The page sends no
//! `Referer`, so a rule bound to its website applies only when the page is known as the source. Without a
//! Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use agent_browser_scene::{content, scene_with, Scene};
use holzi_lib::agent_browser::filters::{LiveFilter, TrackerFilter};
use holzi_lib::mcp_server::grants::SiteLevel;

const LIST: &str = "/ads/\n/blocked.html$document\n||localhost^$image,domain=127.0.0.1\n";

const PARTNER_IMAGE: &str = "/images/logo.gif?partner";

async fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < until, "waited for: {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn filter_on(scene: &Scene) {
    let live = LiveFilter::default();
    live.set(Some(Arc::new(TrackerFilter::new([LIST.to_owned()]))));
    *scene.env.filter.lock().unwrap() = Some(live);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_filter_lists_block_trackers_before_anything_asks() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    filter_on(&scene);

    // The page loads; what the list names never leaves the browser, the rest goes on.
    let page = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/tracked.html") }),
        )
        .await
        .expect("the page opens");
    assert_eq!(content(&page)["title"], "Mit Trackern");
    wait_until("the page's own files", || {
        scene.site.count("GET", "/lib.js") == 1 && scene.site.count("GET", "/images/logo.gif") == 1
    })
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        scene.site.count("GET", "/ads/pixel.gif"),
        0,
        "pixel blocked"
    );
    assert_eq!(
        scene.site.count("GET", "/ads/track.js"),
        0,
        "script blocked"
    );
    assert_eq!(
        scene.site.count("GET", PARTNER_IMAGE),
        0,
        "the rule bound to the page's website applies without a Referer"
    );
    assert!(
        scene.asked.lock().unwrap().is_empty(),
        "a blocked tracker raises no question: {:?}",
        scene.asked.lock().unwrap()
    );

    // A page a list blocks fails with its own code and is never requested.
    let refused = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/blocked.html") }),
        )
        .await
        .expect_err("blocked");
    assert_eq!(refused.code, "blocked_by_filter");
    assert_eq!(scene.site.count("GET", "/blocked.html"), 0);

    scene.browser.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn without_a_filter_nothing_is_blocked() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/tracked.html") }),
        )
        .await
        .expect("the page opens");
    wait_until("the tracker's files", || {
        scene.site.count("GET", "/ads/pixel.gif") == 1
            && scene.site.count("GET", "/ads/track.js") == 1
            && scene.site.count("GET", PARTNER_IMAGE) == 1
    })
    .await;
    scene.browser.stop().await.unwrap();
}

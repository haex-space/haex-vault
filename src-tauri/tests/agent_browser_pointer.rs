//! Spec 065 US8 (spike S3, research R22) against a real Chromium: a drag of a picture, of a link, of
//! a slider and over a canvas that ignores it ends cleanly with interception, the page's own drop
//! receives what was dragged, and no mouse button stays held afterwards. Without a Chromium-based
//! browser the test says so and stops. Spike S4 (a right click) is measured headless here: the page
//! sees `contextmenu`; whether a native menu opens in a window is for the manual check (M18).

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use serde_json::{json, Value};

use agent_browser_scene::{content, scene_with, Scene};
use holzi_lib::mcp_server::grants::SiteLevel;

/// The scale of a screenshot of the tab, taken so that positions are in its pixels.
async fn screenshot_scale(scene: &Scene) -> f64 {
    let shot = scene
        .auto("browser.page.screenshot", json!({ "tab": "t1" }))
        .await
        .expect("a screenshot");
    shot.result["view"]["scale"].as_f64().unwrap()
}

async fn text(scene: &Scene) -> String {
    let page = scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .expect("the page");
    content(&page)["text"].as_str().unwrap().to_owned()
}

fn at(scale: f64, x: f64, y: f64) -> Value {
    json!({ "x": x * scale, "y": y * scale })
}

async fn drag(scene: &Scene, scale: f64, from: (f64, f64), to: (f64, f64)) {
    scene
        .auto(
            "browser.drag",
            json!({
                "tab": "t1",
                "from": at(scale, from.0, from.1),
                "to": at(scale, to.0, to.1),
            }),
        )
        .await
        .unwrap_or_else(|error| panic!("drag {from:?} to {to:?}: {error:?}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_drag_ends_cleanly_and_no_button_stays_held() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/drag.html") }),
        )
        .await
        .expect("open");
    let scale = screenshot_scale(&scene).await;

    // A picture is dragged by the browser itself; interception hands the drop to the page.
    drag(&scene, scale, (60.0, 140.0), (380.0, 150.0)).await;
    let after = text(&scene).await;
    assert!(after.contains("abgelegt: bild"), "{after}");
    assert!(after.contains("buttons: released"), "{after}");

    // A link is dragged the same way; what the page receives is its address.
    drag(&scene, scale, (190.0, 140.0), (380.0, 150.0)).await;
    let after = text(&scene).await;
    assert!(
        after.contains("abgelegt: ") && after.contains("/index.html"),
        "{after}"
    );

    // A slider follows the pointer.
    drag(&scene, scale, (28.0, 270.0), (170.0, 270.0)).await;
    let value: u32 = text(&scene)
        .await
        .lines()
        .find_map(|line| line.trim().parse().ok())
        .unwrap_or(0);
    assert!(value > 30, "the slider moved to {value}");

    // A canvas that ignores the drag, and a drag that starts on nothing, end the same way.
    drag(&scene, scale, (60.0, 360.0), (180.0, 360.0)).await;
    drag(&scene, scale, (500.0, 300.0), (600.0, 300.0)).await;
    assert!(text(&scene).await.contains("buttons: released"));

    // After all of it a plain move over the page shows no button held.
    let hovered = scene
        .auto(
            "browser.hover",
            json!({ "tab": "t1", "x": 600.0 * scale, "y": 100.0 * scale }),
        )
        .await
        .expect("hover");
    assert_eq!(hovered.result["hit"][0]["none"], true, "{}", hovered.result);
    assert!(text(&scene).await.contains("buttons: 0"));
}

//! Spec 065 US9 (FR-065, FR-066) against a real Chromium and the fixture site over HTTPS: the
//! optional cookies of a consent banner are declined before the agent reads the page, and nothing
//! is ever accepted. A banner of a known platform, one of no known platform, one in a shadow root,
//! one that shows late and one in a frame of another website are declined; one that only offers
//! "settings" is left and the agent is told; a dialog that is no banner is not touched; and with
//! the setting off nothing happens. Without a Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use std::time::{Duration, Instant};

use serde_json::json;

use agent_browser_scene::{content, scene_with, Scene};
use holzi_lib::agent_browser::sites::Site;
use holzi_lib::chat::tools::native_action::NativeReply;
use holzi_lib::mcp_server::grants::{SiteGrant, SiteLevel};

async fn open(scene: &Scene, path: &str) -> NativeReply {
    scene
        .auto("browser.open", json!({ "url": scene.site.url(path) }))
        .await
        .unwrap_or_else(|error| panic!("open {path}: {error:?}"))
}

fn text(reply: &NativeReply) -> String {
    content(reply)["text"].as_str().unwrap().to_owned()
}

async fn scene_declining(on: bool) -> Option<Scene> {
    let scene = scene_with(true, SiteLevel::Operate).await?;
    *scene.env.reject_cookies.lock().unwrap() = on;
    Some(scene)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn banners_are_declined_before_the_page_is_read() {
    let Some(scene) = scene_declining(true).await else {
        return;
    };
    for (path, name) in [
        ("/consent-onetrust.html", "a known platform"),
        ("/consent-generic.html", "a banner of no known platform"),
        ("/consent-shadow.html", "a banner in a shadow root"),
        ("/consent-late.html", "a banner that shows late"),
    ] {
        let page = open(&scene, path).await;
        let shown = text(&page);
        assert!(
            shown.contains("abgelehnt") || shown.contains("nur-notwendig"),
            "{name} was declined: {shown}"
        );
        assert!(
            !shown.contains("akzeptiert"),
            "{name} accepted nothing: {shown}"
        );
        assert!(
            !shown.contains("Cookies"),
            "{name}: the banner is gone from what the agent reads: {shown}"
        );
        assert_eq!(
            page.result["consent"], "declined",
            "{name}: {}",
            page.result
        );
    }
    // The platform's own logging request of the choice went out without a question.
    let until = Instant::now() + Duration::from_secs(5);
    while scene.site.count("POST", "/api/consent") == 0 {
        assert!(Instant::now() < until, "the platform logged the choice");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        scene.asked.lock().unwrap().is_empty(),
        "nothing was asked: {:?}",
        scene.asked.lock().unwrap().len()
    );

    // Said once: the next result of the same page does not repeat it.
    let again = scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .unwrap();
    assert!(again.result.get("consent").is_none(), "{}", again.result);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_banner_that_only_offers_settings_is_left_and_the_agent_is_told() {
    let Some(scene) = scene_declining(true).await else {
        return;
    };
    let page = open(&scene, "/consent-settings.html").await;
    assert_eq!(page.result["consent"], "open", "{}", page.result);
    // Neither "accept" nor "save" was pressed, though the settings were opened to look.
    assert!(text(&page).contains("offen"), "{}", text(&page));
    assert!(!text(&page).contains("gespeichert"));
    assert!(!text(&page).contains("akzeptiert"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_dialog_that_is_no_cookie_banner_is_not_touched() {
    let Some(scene) = scene_declining(true).await else {
        return;
    };
    let page = open(&scene, "/consent-other-dialog.html").await;
    assert!(text(&page).contains("offen"), "{}", text(&page));
    assert!(page.result.get("consent").is_none(), "{}", page.result);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_dialog_about_privacy_without_cookies_is_not_touched() {
    let Some(scene) = scene_declining(true).await else {
        return;
    };
    let page = open(&scene, "/consent-privacy-dialog.html").await;
    assert!(text(&page).contains("offen"), "{}", text(&page));
    assert!(page.result.get("consent").is_none(), "{}", page.result);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_button_titled_reject_all_outside_a_banner_is_not_pressed() {
    let Some(scene) = scene_declining(true).await else {
        return;
    };
    let page = open(&scene, "/consent-title-button.html").await;
    assert!(text(&page).contains("offen"), "{}", text(&page));
    assert!(page.result.get("consent").is_none(), "{}", page.result);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_disabled_reject_button_is_pressed_once_it_is_enabled() {
    let Some(scene) = scene_declining(true).await else {
        return;
    };
    let page = open(&scene, "/consent-disabled.html").await;
    // How long the page takes to open depends on the machine, so the button may
    // already be enabled when `open` returns. What must hold at every read: the
    // state says "declined" only when the choice really went through, never
    // because a press on the disabled button was counted.
    let mut read = page;
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        if read.result["consent"] == "declined" {
            assert!(text(&read).contains("nur-notwendig"), "{}", text(&read));
            break;
        }
        assert!(text(&read).contains("offen"), "{}", text(&read));
        assert!(Instant::now() < until, "declined after it was enabled");
        tokio::time::sleep(Duration::from_millis(200)).await;
        read = scene
            .auto("browser.page.read", json!({ "tab": "t1" }))
            .await
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn with_the_setting_off_a_banner_stays() {
    let Some(scene) = scene_declining(false).await else {
        return;
    };
    let page = open(&scene, "/consent-onetrust.html").await;
    assert!(text(&page).contains("Cookies"), "{}", text(&page));
    assert!(text(&page).contains("offen"));
    assert!(page.result.get("consent").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_banner_in_a_frame_of_another_website_is_declined() {
    let Some(scene) = scene_declining(true).await else {
        return;
    };
    // The frame's website need not be one the agent may operate: declining is holzi's own act.
    let partner = Site::parse(&scene.site.partner("/")).unwrap();
    scene.env.rights.lock().unwrap().sites.insert(
        partner,
        SiteGrant {
            level: SiteLevel::Read,
            fill_remembered: false,
        },
    );
    let page = open(&scene, "/consent-frame-host.html").await;
    let mut told = page.result["consent"] == "declined";
    let until = Instant::now() + Duration::from_secs(8);
    while !told {
        assert!(
            Instant::now() < until,
            "the banner in the frame was declined"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        let read = scene
            .auto("browser.page.read", json!({ "tab": "t1" }))
            .await
            .unwrap();
        told = read.result["consent"] == "declined";
    }
}

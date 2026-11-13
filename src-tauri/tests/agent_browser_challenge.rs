//! Spec 065 US10 (FR-067) against a real Chromium and the fixture site over HTTPS: a page that is a
//! check is named with a fixed kind, the checkbox of a check in a frame of another website is an
//! element the agent ticks with `browser.click`, a frame the agent may not operate is named and
//! not listed, and an ordinary page is named nothing. holzi solves nothing itself. Without a
//! Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use std::time::{Duration, Instant};

use serde_json::{json, Value};

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

fn grant_partner(scene: &Scene, level: SiteLevel) {
    let partner = Site::parse(&scene.site.partner("/")).unwrap();
    scene.env.rights.lock().unwrap().sites.insert(
        partner,
        SiteGrant {
            level,
            fill_remembered: false,
        },
    );
}

fn element(page: &NativeReply, name: &str) -> Option<Value> {
    content(page)["elements"]
        .as_array()?
        .iter()
        .find(|element| element["name"] == name)
        .cloned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_waiting_page_of_a_check_is_named_and_an_ordinary_page_is_not() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let waiting = open(&scene, "/challenge-wait.html").await;
    assert_eq!(content(&waiting)["challenge"], "cloudflare");
    let ordinary = open(&scene, "/index.html").await;
    assert!(content(&ordinary).get("challenge").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_checkbox_of_a_check_in_a_frame_is_ticked_like_any_element() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    grant_partner(&scene, SiteLevel::Operate);
    let page = open(&scene, "/challenge-host.html").await;
    assert_eq!(content(&page)["challenge"], "recaptcha", "{}", page.result);
    let robot = element(&page, "Ich bin kein Roboter").expect("the checkbox of the check");
    assert_eq!(robot["kind"], "checkbox");
    assert!(robot.get("frame").is_some(), "it is an element of a frame");

    let ticked = scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": robot["n"] }),
        )
        .await
        .expect("ticked");
    assert_eq!(
        element(&ticked, "Ich bin kein Roboter").unwrap()["checked"],
        true
    );
    // The page is told by the frame and goes on.
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let read = scene
            .auto("browser.page.read", json!({ "tab": "t1" }))
            .await
            .unwrap();
        if content(&read)["text"]
            .as_str()
            .unwrap()
            .contains("Prüfung bestanden")
        {
            break;
        }
        assert!(Instant::now() < until, "the page went on");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(scene.asked.lock().unwrap().is_empty(), "nothing asked");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_check_the_agent_may_not_operate_is_named_and_its_website_is_said() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    grant_partner(&scene, SiteLevel::Read);
    let page = open(&scene, "/challenge-host.html").await;
    assert_eq!(content(&page)["challenge"], "recaptcha");
    assert!(element(&page, "Ich bin kein Roboter").is_none());
    let named = content(&page)["frames_not_operable"].as_array().unwrap();
    assert!(
        named
            .iter()
            .any(|site| site.as_str().unwrap().contains("localhost")),
        "{named:?}"
    );
}

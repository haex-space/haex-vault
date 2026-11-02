//! Spec 065 T062 (US4) against a real Chromium and the fixture site over `https`: an agent with the
//! rights group "Passwörter" has holzi fill a login into a page that copies what was typed into its
//! own text; the agent never sees the password in a result, and screenshots stay off. The first
//! fill asks; "Für diese Website merken" lets the next one go without asking; two matching entries
//! ask every time; without the rights group there is no entry. Without a Chromium-based browser
//! the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use serde_json::json;

use agent_browser_scene::{content, scene_with, Scene};
use holzi_lib::agent_browser::prompts::BrowserPromptKind;
use holzi_lib::agent_browser::sites::Site;
use holzi_lib::caller::Caller;
use holzi_lib::mcp_server::grants::{PasswordLevel, PasswordRights, SiteLevel};
use holzi_lib::passwords::model::ItemInput;
use holzi_lib::passwords::service::PasswordsService;

const MARKER: &str = "Marker-7f3a-Geheim";

async fn entry(scene: &Scene, title: &str, password: &str) {
    PasswordsService::new(scene.env.db.clone())
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some(title.to_owned()),
                url: Some(scene.site.url("/login.html")),
                username: Some("anna@example.org".to_owned()),
                password: Some(password.to_owned()),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("an entry");
}

fn fills_asked(scene: &Scene) -> usize {
    scene
        .asked
        .lock()
        .unwrap()
        .iter()
        .filter(|question| question.kind == BrowserPromptKind::Fill)
        .count()
}

async fn login_tab(scene: &Scene) -> String {
    let page = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/login.html") }),
        )
        .await
        .expect("the login page");
    page.result["tab"].as_str().unwrap().to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn holzi_fills_a_login_the_agent_never_sees() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    entry(&scene, "Shop", MARKER).await;
    let tab = login_tab(&scene).await;

    // Without the rights group "Passwörter" no entry is there for the agent.
    let none = scene
        .auto("browser.credentials.fill", json!({ "tab": tab }))
        .await
        .expect_err("no rights group");
    assert_eq!(none.code, "no_matching_entry");

    *scene.env.passwords.lock().unwrap() = Some(PasswordRights {
        level: PasswordLevel::Read,
        tags: None,
    });
    *scene.allow.lock().unwrap() = true;
    *scene.remember.lock().unwrap() = Some("fill");
    let filled = scene
        .auto("browser.credentials.fill", json!({ "tab": tab }))
        .await
        .expect("filled");
    assert_eq!(filled.result["filled"], true, "{}", filled.result);
    assert_eq!(fills_asked(&scene), 1);
    let question = scene.asked.lock().unwrap().last().cloned().unwrap();
    assert_eq!(question.entries.len(), 1);
    assert_eq!(question.entries[0].title, "Shop");

    // The page shows the password in its text; the agent sees it nowhere.
    let page = scene
        .auto("browser.page.read", json!({ "tab": tab }))
        .await
        .expect("the page");
    let text = content(&page)["text"].as_str().unwrap();
    assert!(text.contains("Eingegeben: "), "{text}");
    assert!(!page.result.to_string().contains(MARKER), "{}", page.result);
    assert!(!page.result.to_string().contains("anna@example.org"));

    let screenshot = scene
        .auto("browser.page.screenshot", json!({ "tab": tab }))
        .await
        .expect_err("screenshots are off");
    assert_eq!(screenshot.code, "screenshot_blocked");

    // Remembered: the next fill on this website does not ask.
    let site = Site::parse(&scene.site.origin()).unwrap();
    assert!(scene.env.rights.lock().unwrap().sites[&site].fill_remembered);
    scene
        .auto("browser.credentials.fill", json!({ "tab": tab }))
        .await
        .expect("filled again");
    assert_eq!(fills_asked(&scene), 1);

    // Two matching entries ask every time.
    entry(&scene, "Shop (alt)", "Marker-alt-2").await;
    scene
        .auto("browser.credentials.fill", json!({ "tab": tab }))
        .await
        .expect_err("no entry chosen");
    assert_eq!(fills_asked(&scene), 2);
    let question = scene.asked.lock().unwrap().last().cloned().unwrap();
    assert_eq!(question.entries.len(), 2);
}

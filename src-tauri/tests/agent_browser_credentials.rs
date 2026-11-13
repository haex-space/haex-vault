//! Spec 065 T062 (US4) against a real Chromium and the fixture site over `https`: an agent with the
//! rights group "Passwörter" has holzi fill a login into a page that copies what was typed into its
//! own text; the agent never sees the password in a result, and screenshots stay off. The first
//! fill asks; "Für diese Website merken" lets the next one go without asking; two matching entries
//! ask every time; without the rights group there is no entry. A new login (FR-036) gets a password
//! holzi generates into the new field and its repeat field; "Speichern" stores it as an entry,
//! "Ablehnen" empties the fields, and reading rights alone save nothing. Without a Chromium-based
//! browser the tests say so and stop.

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
use holzi_lib::chat::tools::native_action::NativeReply;
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

fn asked(scene: &Scene, kind: BrowserPromptKind) -> usize {
    scene
        .asked
        .lock()
        .unwrap()
        .iter()
        .filter(|question| question.kind == kind)
        .count()
}

fn fills_asked(scene: &Scene) -> usize {
    asked(scene, BrowserPromptKind::Fill)
}

/// The number of the element whose name contains `name`.
fn element_named(page: &NativeReply, name: &str) -> u64 {
    content(page)["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["name"].as_str().is_some_and(|n| n.contains(name)))
        .unwrap_or_else(|| panic!("no element {name}: {}", page.result))["n"]
        .as_u64()
        .unwrap()
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
    // A screenshot before the fill leaves a view, so positions could be used.
    scene
        .auto("browser.page.screenshot", json!({ "tab": tab }))
        .await
        .expect("a screenshot before anything is filled");

    // Without the rights group "Passwörter" no entry is there for the agent.
    let none = scene
        .auto("browser.credentials.fill", json!({ "tab": tab }))
        .await
        .expect_err("no rights group");
    assert_eq!(none.code, "no_matching_entry");

    *scene.env.passwords.lock().unwrap() = Some(PasswordRights {
        level: PasswordLevel::Read,
        tags: None,
        default_tag: None,
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
    // The refused screenshot also took the view: no position is used afterwards (FR-022, FR-058).
    let position = scene
        .auto("browser.click_at", json!({ "tab": tab, "x": 10, "y": 10 }))
        .await
        .expect_err("no view after a refused screenshot");
    assert_eq!(position.code, "no_view");

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn holzi_creates_a_login_the_agent_never_sees() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let address = scene.site.url("/signup.html");
    let page = scene
        .auto("browser.open", json!({ "url": address }))
        .await
        .expect("the signup page");
    let tab = page.result["tab"].as_str().unwrap().to_owned();
    let create = json!({
        "tab": tab,
        "element": element_named(&page, "Neues Passwort"),
        "username": "anna@example.org",
    });
    let service = PasswordsService::new(scene.env.db.clone());
    let page_url = url::Url::parse(&address).unwrap();
    let entries = || async {
        service
            .headers_for_page(&Caller::User, &[], &page_url)
            .await
            .expect("the entries")
    };
    let read = || async {
        scene
            .auto("browser.page.read", json!({ "tab": tab }))
            .await
            .expect("the page")
    };

    // Reading rights alone save nothing and generate nothing.
    *scene.env.passwords.lock().unwrap() = Some(PasswordRights {
        level: PasswordLevel::Read,
        tags: None,
        default_tag: None,
    });
    let refused = scene
        .auto("browser.credentials.create", create.clone())
        .await
        .expect_err("reading only");
    assert_eq!(refused.code, "no_write_rights");
    assert_eq!(asked(&scene, BrowserPromptKind::NewCredentials), 0);

    // "Ablehnen" empties both fields and stores nothing.
    *scene.env.passwords.lock().unwrap() = Some(PasswordRights {
        level: PasswordLevel::ReadWrite,
        tags: None,
        default_tag: None,
    });
    let declined = scene
        .auto("browser.credentials.create", create.clone())
        .await
        .expect_err("declined");
    assert_eq!(declined.code, "user_declined");
    let question = scene.asked.lock().unwrap().last().cloned().unwrap();
    assert_eq!(question.kind, BrowserPromptKind::NewCredentials);
    assert_eq!(question.label.as_deref(), Some("anna@example.org"));
    let text = content(&read().await)["text"].as_str().unwrap().to_owned();
    assert!(text.contains("/ leer"), "{text}");
    assert!(entries().await.is_empty());

    // "Speichern": both fields hold the same new password, the entry holds it, the agent never.
    *scene.allow.lock().unwrap() = true;
    let created = scene
        .auto("browser.credentials.create", create)
        .await
        .expect("created");
    assert_eq!(created.result["created"], true, "{}", created.result);
    let headers = entries().await;
    assert_eq!(headers.len(), 1);
    let item = service
        .read_secret_item(&Caller::User, &[], headers[0].id.clone())
        .await
        .expect("the new entry");
    let password = item.password.clone().expect("a password");
    assert_eq!(password.chars().count(), 20);
    assert_eq!(item.username.as_deref(), Some("anna@example.org"));
    let page = read().await;
    let text = content(&page)["text"].as_str().unwrap();
    assert!(text.contains("/ gleich"), "{text}");
    assert!(
        !page.result.to_string().contains(&password),
        "{}",
        page.result
    );
    assert!(!created.result.to_string().contains(&password));

    let screenshot = scene
        .auto("browser.page.screenshot", json!({ "tab": tab }))
        .await
        .expect_err("screenshots are off");
    assert_eq!(screenshot.code, "screenshot_blocked");
}

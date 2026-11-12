//! Spec 065 T113 (stage A) against a real Chromium and the fixture site over HTTPS: keys, hover and
//! the mouse wheel (US7). A key goes under the rules of a click (FR-062): Enter on a button that
//! submits asks, a refused key never reaches the page, a website that may only be read allows
//! hovering and the wheel but no key. Without a Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
#[allow(dead_code)]
mod agent_browser_fixture;
#[path = "common/agent_browser_scene.rs"]
mod agent_browser_scene;

use serde_json::{json, Value};

use agent_browser_scene::{content, scene_with, Scene};
use holzi_lib::agent_browser::prompts::BrowserPromptKind;
use holzi_lib::chat::tools::native_action::NativeReply;
use holzi_lib::chat::tools::permission::PermissionMode;
use holzi_lib::mcp_server::grants::SiteLevel;

fn find(reply: &NativeReply, name: &str) -> Option<Value> {
    content(reply)["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["name"] == name)
        .cloned()
}

fn element(reply: &NativeReply, name: &str) -> Value {
    find(reply, name).unwrap_or_else(|| panic!("no element {name}: {}", reply.result))
}

fn text(reply: &NativeReply) -> String {
    content(reply)["text"].as_str().unwrap().to_owned()
}

async fn open(scene: &Scene, path: &str) -> NativeReply {
    scene
        .auto("browser.open", json!({ "url": scene.site.url(path) }))
        .await
        .unwrap_or_else(|error| panic!("open {path}: {error:?}"))
}

async fn press(scene: &Scene, input: Value) -> Result<NativeReply, String> {
    scene
        .auto("browser.press_key", input)
        .await
        .map_err(|error| error.code.to_string())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn keys_work_under_the_rules_of_a_click() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let page = open(&scene, "/keys.html").await;
    let note = element(&page, "Notiz")["n"].clone();

    // A key goes to the element named, and the result says where it went.
    let typed = press(&scene, json!({ "tab": "t1", "key": "a", "element": note }))
        .await
        .expect("a key");
    assert_eq!(element(&typed, "Notiz")["value"], "a");
    assert_eq!(typed.result["focus"]["name"], "Notiz");
    assert!(text(&typed).contains("a;"), "{}", text(&typed));

    // Select all is allowed and arrives as a shortcut.
    let chord = press(
        &scene,
        json!({ "tab": "t1", "key": "a", "modifiers": ["ctrl"] }),
    )
    .await
    .expect("select all");
    assert!(text(&chord).contains("a+ctrl;"), "{}", text(&chord));

    // Keys that act on the clipboard, the browser or a window are refused before the page sees them.
    for (key, modifiers) in [
        ("v", json!(["ctrl"])),
        ("c", json!(["ctrl"])),
        ("t", json!(["ctrl"])),
        ("n", json!(["ctrl"])),
        ("w", json!(["ctrl"])),
        ("ArrowLeft", json!(["alt"])),
        ("F12", json!([])),
        ("F5", json!([])),
    ] {
        let refused = press(
            &scene,
            json!({ "tab": "t1", "key": key, "modifiers": modifiers }),
        )
        .await
        .expect_err(key);
        assert_eq!(refused, "key_not_allowed", "{key} {modifiers}");
    }
    let tabs = scene.auto("browser.tabs.list", json!({})).await.unwrap();
    assert_eq!(
        content(&tabs)["tabs"].as_array().unwrap().len(),
        1,
        "no tab was opened"
    );
    let after = scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .unwrap();
    for never in ["v+ctrl", "c+ctrl", "F12", "F5"] {
        assert!(!text(&after).contains(never), "{never}: {}", text(&after));
    }

    // A count presses more than once; one count over twenty is invalid.
    let page = open(&scene, "/keys.html").await;
    let moved = press(
        &scene,
        json!({ "tab": "t1", "key": "ArrowRight", "count": 3 }),
    )
    .await
    .expect("a key three times");
    assert!(text(&moved).contains("x=3"), "{}", text(&moved));
    assert_eq!(moved.result["focus"]["name"], "Feld");
    let too_many = press(
        &scene,
        json!({ "tab": "t1", "key": "ArrowRight", "count": 21 }),
    )
    .await
    .expect_err("too many");
    assert_eq!(too_many, "invalid_input");

    // Typing as keys: one key event per character, for pages that listen to keydown.
    let note = element(&page, "Notiz")["n"].clone();
    let keyed = scene
        .auto(
            "browser.type",
            json!({ "tab": "t1", "element": note, "text": "Hallo", "mode": "keys" }),
        )
        .await
        .expect("typed as keys");
    assert_eq!(element(&keyed, "Notiz")["value"], "Hallo");
    assert!(text(&keyed).contains("H;a;l;l;o;"), "{}", text(&keyed));
    let bad = scene
        .auto(
            "browser.type",
            json!({ "tab": "t1", "element": note, "text": "a\nb", "mode": "keys" }),
        )
        .await
        .expect_err("a line break");
    assert_eq!(bad.code, "invalid_input");

    // Enter in a search field sends no personal data: it asks nothing in Auto.
    let query = element(&page, "Suche")["n"].clone();
    press(
        &scene,
        json!({ "tab": "t1", "key": "Enter", "element": query }),
    )
    .await
    .expect("Enter in a search field");
    assert!(scene.asked.lock().unwrap().is_empty());

    // Enter on a button that submits asks as a click on it does; declined, nothing is sent.
    let page = open(&scene, "/keys.html").await;
    let order = element(&page, "Bestellen")["n"].clone();
    let declined = press(
        &scene,
        json!({ "tab": "t1", "key": "Enter", "element": order }),
    )
    .await
    .expect_err("declined");
    assert_eq!(declined, "user_declined");
    let question = scene.asked.lock().unwrap().last().cloned().unwrap();
    assert_eq!(question.kind, BrowserPromptKind::Submit);
    assert_eq!(question.label.as_deref(), Some("Bestellen"));
    assert_eq!(scene.site.count("POST", "/send"), 0, "nothing was sent");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hovering_opens_a_menu_and_the_wheel_loads_a_list() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let page = open(&scene, "/hover.html").await;
    assert!(find(&page, "Abmelden").is_none(), "the menu is closed");
    let trigger = element(&page, "Konto")["n"].clone();
    let hovered = scene
        .auto("browser.hover", json!({ "tab": "t1", "element": trigger }))
        .await
        .expect("hovered");
    assert!(find(&hovered, "Abmelden").is_some(), "{}", hovered.result);
    assert!(
        scene.asked.lock().unwrap().is_empty(),
        "hovering asks nothing"
    );

    open(&scene, "/wheel.html").await;
    let mut moves = 0;
    let mut last = Value::Null;
    for _ in 0..30 {
        let reply = scene
            .auto(
                "browser.scroll",
                json!({ "tab": "t1", "direction": "down", "amount": "page" }),
            )
            .await
            .expect("wheel");
        last = reply.result["moved"].clone();
        if last == Value::Bool(false) {
            assert!(text(&reply).contains("Zeile 40"), "{}", text(&reply));
            break;
        }
        assert_eq!(last, Value::Bool(true));
        moves += 1;
    }
    assert_eq!(last, Value::Bool(false), "the list ends");
    assert!(moves >= 3, "the list grew while it was scrolled: {moves}");

    let pixels = scene
        .auto(
            "browser.scroll",
            json!({ "tab": "t1", "direction": "up", "amount": 100 }),
        )
        .await
        .expect("wheel up by pixels");
    assert_eq!(pixels.result["moved"], true);
    for bad in [json!(0), json!(5001), json!("far")] {
        let refused = scene
            .auto(
                "browser.scroll",
                json!({ "tab": "t1", "direction": "down", "amount": bad }),
            )
            .await
            .expect_err("amount");
        assert_eq!(refused.code, "invalid_input");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_website_that_may_only_be_read_allows_hover_and_the_wheel_but_no_key() {
    let Some(scene) = scene_with(true, SiteLevel::Read).await else {
        return;
    };
    let page = open(&scene, "/keys.html").await;
    let note = element(&page, "Notiz")["n"].clone();
    for input in [
        json!({ "tab": "t1", "key": "a", "element": note }),
        json!({ "tab": "t1", "key": "Enter" }),
    ] {
        let refused = press(&scene, input).await.expect_err("read only");
        assert_eq!(refused, "read_only_site");
    }
    let typed = scene
        .auto(
            "browser.type",
            json!({ "tab": "t1", "element": note, "text": "x", "mode": "keys" }),
        )
        .await
        .expect_err("typing");
    assert_eq!(typed.code, "read_only_site");

    let hover = open(&scene, "/hover.html").await;
    let trigger = element(&hover, "Konto")["n"].clone();
    scene
        .auto("browser.hover", json!({ "tab": "t1", "element": trigger }))
        .await
        .expect("hover on a read-only website");
    open(&scene, "/wheel.html").await;
    let scrolled = scene
        .auto(
            "browser.scroll",
            json!({ "tab": "t1", "direction": "down" }),
        )
        .await
        .expect("wheel on a read-only website");
    assert_eq!(scrolled.result["moved"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manuell_asks_before_a_key_and_plan_refuses_it() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    open(&scene, "/keys.html").await;
    let input = json!({ "tab": "t1", "key": "ArrowRight" });

    let refused = scene
        .call("browser.press_key", input.clone(), PermissionMode::Plan)
        .await
        .expect_err("plan");
    assert_eq!(refused.code, "plan_mode");

    let before = scene.asked.lock().unwrap().len();
    let declined = scene
        .call("browser.press_key", input.clone(), PermissionMode::Manual)
        .await
        .expect_err("manual, declined");
    assert_eq!(declined.code, "user_declined");
    assert_eq!(scene.asked.lock().unwrap().len(), before + 1);

    *scene.allow.lock().unwrap() = true;
    let allowed = scene
        .call("browser.press_key", input, PermissionMode::Manual)
        .await
        .expect("manual, allowed");
    assert!(text(&allowed).contains("x=1"), "{}", text(&allowed));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_page_that_goes_fullscreen_is_sent_back() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let page = open(&scene, "/fullscreen.html").await;
    let button = element(&page, "Vollbild")["n"].clone();
    let clicked = scene
        .auto("browser.click", json!({ "tab": "t1", "element": button }))
        .await
        .expect("clicked");
    // The marker leaves fullscreen at once and tells holzi; the next result says so (FR-055).
    let mut told = clicked.result["fullscreen_refused"] == true;
    let mut shown = text(&clicked);
    for _ in 0..20 {
        if told && shown.contains("an;aus;") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let read = scene
            .auto("browser.page.read", json!({ "tab": "t1" }))
            .await
            .expect("read");
        told |= read.result["fullscreen_refused"] == true;
        shown = text(&read);
    }
    assert!(shown.contains("an;aus;"), "entered and left: {shown}");
    assert!(told, "the agent was told");
}

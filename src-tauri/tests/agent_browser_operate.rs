//! Spec 065 T046 against a real Chromium and the fixture site over HTTPS: an agent types, chooses
//! and clicks with real input events. In Auto, operating runs without a question and submitting
//! asks: a declined submit sends nothing, an allowed one sends its request once, and a request a
//! page sends later on its own asks again; no time lets it through. In Manuell every operating
//! action asks once; in Plan it is refused; a website allowed only for reading refuses operating.
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
use holzi_lib::chat::tools::native_action::NativeReply;
use holzi_lib::chat::tools::permission::PermissionMode;
use holzi_lib::mcp_server::grants::SiteLevel;

/// The number of the element named `name` on the page of `reply`.
fn element(reply: &NativeReply, name: &str) -> Value {
    content(reply)["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["name"] == name)
        .unwrap_or_else(|| panic!("no element {name}: {}", reply.result))
        .clone()
}

fn asked(scene: &Scene) -> Vec<(BrowserPromptKind, String)> {
    scene
        .asked
        .lock()
        .unwrap()
        .iter()
        .map(|q| (q.kind, q.label.clone().unwrap_or_default()))
        .collect()
}

async fn form(scene: &Scene) -> NativeReply {
    scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/form.html") }),
        )
        .await
        .expect("the form")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_agent_operates_and_every_submit_asks() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };

    // Step 1 marks what submits.
    let page = form(&scene).await;
    assert_eq!(element(&page, "Weiter")["submits"], true);
    for plain in ["Details zeigen", "Zum Artikel", "Versandkosten", "Name"] {
        assert!(
            element(&page, plain).get("submits").is_none(),
            "{plain} does not submit"
        );
    }
    let n = |name: &str| element(&page, name)["n"].clone();

    // In Auto, typing, choosing and clicking run without a question.
    let typed = scene
        .auto(
            "browser.type",
            json!({ "tab": "t1", "element": n("Name"), "text": "Ana" }),
        )
        .await
        .expect("typed");
    assert_eq!(element(&typed, "Name")["value"], "Ana");
    let chosen = scene
        .auto(
            "browser.select",
            json!({ "tab": "t1", "element": n("Menge"), "value": "2" }),
        )
        .await
        .expect("chosen");
    assert_eq!(element(&chosen, "Menge")["value"], "2");
    let clicked = scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": n("Details zeigen") }),
        )
        .await
        .expect("clicked");
    assert!(content(&clicked)["text"]
        .as_str()
        .unwrap()
        .contains("Geöffnet"));
    assert!(
        asked(&scene).is_empty(),
        "nothing asked: {:?}",
        asked(&scene)
    );

    // A submit asks; declined, nothing is sent.
    let declined = scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": n("Weiter") }),
        )
        .await
        .expect_err("declined");
    assert_eq!(declined.code, "user_declined");
    let question = scene.asked.lock().unwrap().last().cloned().unwrap();
    assert_eq!(question.kind, BrowserPromptKind::Submit);
    assert_eq!(question.label.as_deref(), Some("Weiter"));
    let fields: Vec<(String, String)> = question
        .fields
        .iter()
        .map(|f| (f.name.clone(), f.value.clone()))
        .collect();
    assert!(
        fields.contains(&("name".to_owned(), "Ana".to_owned())),
        "{fields:?}"
    );
    assert!(
        fields.contains(&("password".to_owned(), "•••".to_owned())),
        "{fields:?}"
    );
    assert_eq!(scene.site.count("POST", "/send"), 0, "nothing was sent");

    // Allowed, its request goes once, without a second question.
    *scene.allow.lock().unwrap() = true;
    let before = asked(&scene).len();
    let sent = scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": n("Weiter") }),
        )
        .await
        .expect("sent");
    assert_eq!(content(&sent)["title"], "Gesendet");
    assert_eq!(scene.site.count("POST", "/send"), 1);
    assert_eq!(
        asked(&scene).len(),
        before + 1,
        "one question for one submit"
    );

    // A request the page sends later on its own asks again; declined, it never arrives.
    *scene.allow.lock().unwrap() = false;
    let page = form(&scene).await;
    scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": element(&page, "Merken")["n"] }),
        )
        .await
        .expect("clicked");
    let until = Instant::now() + Duration::from_secs(10);
    while !asked(&scene)
        .iter()
        .any(|(kind, label)| *kind == BrowserPromptKind::Submit && label.ends_with("/api"))
    {
        assert!(
            Instant::now() < until,
            "the late request asked: {:?}",
            asked(&scene)
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        scene.site.count("POST", "/api"),
        0,
        "the late request was held"
    );

    // Enter in a field submits and asks.
    let page = form(&scene).await;
    let entered = scene
        .auto(
            "browser.type",
            json!({ "tab": "t1", "element": element(&page, "Name")["n"], "text": "Bo", "submit": true }),
        )
        .await
        .expect_err("declined");
    assert_eq!(entered.code, "user_declined");
    assert_eq!(
        scene.site.count("POST", "/send"),
        1,
        "still only the allowed one"
    );

    // In Manuell, operating asks once; in Plan it is refused.
    let page = form(&scene).await;
    let before = asked(&scene).len();
    let manual = scene
        .call(
            "browser.click",
            json!({ "tab": "t1", "element": element(&page, "Details zeigen")["n"] }),
            PermissionMode::Manual,
        )
        .await
        .expect_err("declined");
    assert_eq!(manual.code, "user_declined");
    assert_eq!(asked(&scene)[before].0, BrowserPromptKind::Action);
    assert_eq!(asked(&scene).len(), before + 1);
    let plan = scene
        .call(
            "browser.click",
            json!({ "tab": "t1", "element": element(&page, "Details zeigen")["n"] }),
            PermissionMode::Plan,
        )
        .await
        .expect_err("refused");
    assert_eq!(plan.code, "plan_mode");

    // A website allowed only for reading refuses operating.
    for grant in scene.env.rights.lock().unwrap().sites.values_mut() {
        grant.level = SiteLevel::Read;
    }
    let read_only = scene
        .auto(
            "browser.click",
            json!({ "tab": "t1", "element": element(&page, "Details zeigen")["n"] }),
        )
        .await
        .expect_err("read only");
    assert_eq!(read_only.code, "read_only_site");

    scene.browser.stop().await.unwrap();
}

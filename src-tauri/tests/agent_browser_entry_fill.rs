//! Spec 065 T083 (US4, FR-037, FR-038) against a real Chromium and the fixture site over `https`:
//! an agent has holzi fill bank data of an entry the user chooses into a checkout that copies the
//! values into its own text, its card field in a frame of the payment provider (T091). The agent
//! never sees a value; a frame of a website the agent may only read is not filled; a deselected
//! row stays empty; submitting asks with `•••`; every call asks again. Without the rights group
//! there is no entry.
//! Without a Chromium-based browser the test says so and stops.

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
use holzi_lib::mcp_server::grants::{PasswordLevel, PasswordRights, SiteGrant, SiteLevel};
use holzi_lib::passwords::model::{ItemInput, KeyValueInput};
use holzi_lib::passwords::service::PasswordsService;

const IBAN: &str = "DE89370400440532013000";
const CARD: &str = "4111111111111111";

fn asked(scene: &Scene, kind: BrowserPromptKind) -> usize {
    scene
        .asked
        .lock()
        .unwrap()
        .iter()
        .filter(|question| question.kind == kind)
        .count()
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn holzi_fills_bank_data_the_agent_never_sees() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    PasswordsService::new(scene.env.db.clone())
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Girokonto".to_owned()),
                key_values: vec![
                    KeyValueInput {
                        key: "IBAN".to_owned(),
                        value: Some(IBAN.to_owned()),
                    },
                    KeyValueInput {
                        key: "Kartennummer".to_owned(),
                        value: Some(CARD.to_owned()),
                    },
                ],
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("an entry");
    // The payment provider's frame loads: the agent may read its website.
    let partner = Site::parse(&scene.site.partner("/")).unwrap();
    let grant = |level| SiteGrant {
        level,
        fill_remembered: false,
    };
    scene
        .env
        .rights
        .lock()
        .unwrap()
        .sites
        .insert(partner.clone(), grant(SiteLevel::Read));
    let page = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/checkout.html") }),
        )
        .await
        .expect("the checkout");
    let tab = page.result["tab"].as_str().unwrap().to_owned();
    let fill = json!({ "tab": tab, "purpose": "Lastschrift für die Bestellung" });

    // Without the rights group "Passwörter" no entry is there for the agent.
    let none = scene
        .auto("browser.entry.fill", fill.clone())
        .await
        .expect_err("no rights group");
    assert_eq!(none.code, "no_matching_entry");

    *scene.env.passwords.lock().unwrap() = Some(PasswordRights {
        level: PasswordLevel::Read,
        tags: None,
        default_tag: None,
    });
    *scene.allow.lock().unwrap() = true;
    let last_question = |scene: &Scene| {
        scene
            .asked
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("a question")
    };
    let keys_asked = |scene: &Scene| -> Vec<String> {
        last_question(scene)
            .mapping
            .iter()
            .map(|row| row.key.clone())
            .collect()
    };
    let iban = element_named(&page, "IBAN");

    // The payment frame's website may only be read: its card field is neither offered nor filled.
    let filled = scene
        .auto("browser.entry.fill", fill.clone())
        .await
        .expect("filled");
    assert_eq!(filled.result["filled"], json!([iban]), "{}", filled.result);
    assert!(
        filled.result.get("in_frames").is_none(),
        "{}",
        filled.result
    );
    let reply = filled.result.to_string();
    for hidden in [IBAN, CARD, "Girokonto", "Kartennummer"] {
        assert!(!reply.contains(hidden), "{reply}");
    }
    let question = last_question(&scene);
    assert_eq!(question.kind, BrowserPromptKind::EntryFill);
    assert_eq!(
        question.reason.as_deref(),
        Some("Lastschrift für die Bestellung")
    );
    assert_eq!(question.entries.len(), 1);
    assert_eq!(keys_asked(&scene), ["IBAN"]);
    assert!(!serde_json::to_string(&question).unwrap().contains(IBAN));

    // The page shows the IBAN in its text; the agent sees it nowhere.
    let page = scene
        .auto("browser.page.read", json!({ "tab": tab }))
        .await
        .expect("the page");
    let text = content(&page)["text"].as_str().unwrap();
    assert!(text.contains("IBAN: •••"), "{text}");
    assert!(text.contains("Karte: leer"), "{text}");
    assert!(!page.result.to_string().contains(IBAN));

    // Submitting asks and shows the filled value as a placeholder.
    *scene.allow.lock().unwrap() = false;
    let button = element_named(&page, "Zahlungspflichtig");
    let _ = scene
        .auto("browser.click", json!({ "tab": tab, "element": button }))
        .await;
    let submit = last_question(&scene);
    assert_eq!(submit.kind, BrowserPromptKind::Submit);
    let shown: Vec<(String, String)> = submit
        .fields
        .iter()
        .map(|field| (field.name.clone(), field.value.clone()))
        .collect();
    assert!(
        shown.contains(&("iban".to_owned(), "•••".to_owned())),
        "{shown:?}"
    );

    // Operable, the frame's card field is offered with its website; deselected, it stays empty.
    scene
        .env
        .rights
        .lock()
        .unwrap()
        .sites
        .insert(partner, grant(SiteLevel::Operate));
    *scene.allow.lock().unwrap() = true;
    scene
        .extra
        .lock()
        .unwrap()
        .insert("skip".to_owned(), json!(["e1.1.0"]));
    let deselected = scene
        .auto("browser.entry.fill", fill.clone())
        .await
        .expect("filled without the card");
    assert!(deselected.result.get("in_frames").is_none());
    assert_eq!(keys_asked(&scene), ["IBAN", "Kartennummer"]);
    let card_row = last_question(&scene).mapping[1].clone();
    assert_eq!(card_row.id, "e1.1.0");
    assert!(
        card_row
            .frame
            .as_deref()
            .is_some_and(|site| site.contains("localhost")),
        "{card_row:?}"
    );

    // Kept, holzi fills the card number into the frame; every call asked again.
    scene.extra.lock().unwrap().clear();
    let both = scene
        .auto("browser.entry.fill", fill)
        .await
        .expect("filled with the card");
    assert_eq!(both.result["in_frames"], 1, "{}", both.result);
    assert!(!both.result.to_string().contains(CARD));
    scene
        .auto(
            "browser.wait",
            json!({ "tab": tab, "for": "text", "text": "Karte: gefüllt" }),
        )
        .await
        .expect("the frame reported the card");
    assert_eq!(asked(&scene, BrowserPromptKind::EntryFill), 3);

    let screenshot = scene
        .auto("browser.page.screenshot", json!({ "tab": tab }))
        .await
        .expect_err("screenshots are off");
    assert_eq!(screenshot.code, "screenshot_blocked");
}

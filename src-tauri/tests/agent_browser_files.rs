//! Spec 065 T067 (US5) against a real Chromium and the fixture site over `https`: a download lands
//! in "Agenten-Downloads" of the vault under the name the website gave it; one of 150 MB asks the
//! user and, declined, is cancelled and never reaches the agent; an upload without file rights is
//! refused alike for a file that is there and one that is not, and with them it asks like a submit,
//! puts the file into the field and sends nothing by itself. Without a Chromium-based browser the
//! test says so and stops.

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
use holzi_lib::files::access::{AgentGrants, DeviceGrant};
use holzi_lib::mcp_server::grants::SiteLevel;

/// The number of the element named `name` on the page of `reply`.
fn element(reply: &NativeReply, name: &str) -> Value {
    content(reply)["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["name"] == name)
        .unwrap_or_else(|| panic!("no element {name}: {}", reply.result))["n"]
        .clone()
}

async fn files_page(scene: &Scene) -> NativeReply {
    scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/files.html") }),
        )
        .await
        .expect("the files page")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_agent_downloads_and_uploads_within_its_rights() {
    let Some(scene) = scene_with(true, SiteLevel::Operate).await else {
        return;
    };
    let page = files_page(&scene).await;
    let tab = page.result["tab"].as_str().unwrap().to_owned();
    let folder = scene
        .env
        .downloads
        .path()
        .join("Agenten-Downloads")
        .join("read");

    // A download: the file lands in the vault's folder under the website's name.
    let invoice = element(&page, "Rechnung herunterladen");
    let kept = scene
        .auto(
            "browser.download",
            json!({ "tab": tab, "element": invoice }),
        )
        .await
        .expect("the invoice");
    assert_eq!(kept.result["name"], "Rechnung 2026-11.txt");
    assert_eq!(kept.result["type"], "text/plain");
    let path = std::path::PathBuf::from(kept.result["path"].as_str().unwrap());
    assert_eq!(path, folder.join("Rechnung 2026-11.txt"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "Rechnung Nr. 4711\n"
    );
    assert_eq!(kept.result["size"], 18);
    assert!(scene.asked.lock().unwrap().is_empty(), "Auto asks no click");

    // 150 MB: the user is asked; declined, the agent gets nothing.
    let big = element(&page, "Großen Download starten");
    let declined = scene
        .auto("browser.download", json!({ "tab": tab, "element": big }))
        .await
        .expect_err("declined");
    assert_eq!(declined.code, "user_declined");
    let asked = scene.asked.lock().unwrap().clone();
    let question = asked
        .iter()
        .find(|question| question.kind == BrowserPromptKind::Download)
        .expect("the download question");
    assert_eq!(question.label.as_deref(), Some("gross.bin"));
    assert_eq!(question.bytes, Some(150_000_000));
    assert!(!folder.join("gross.bin").exists());

    // An upload without file rights: the same answer whether the file is there or not.
    let source = tempfile::tempdir().unwrap();
    let receipt = source.path().join("Beleg.pdf");
    std::fs::write(&receipt, "%PDF-1.4").unwrap();
    let field = element(&page, "Beleg");
    for path in [receipt.clone(), source.path().join("missing.pdf")] {
        let refused = scene
            .auto(
                "browser.upload",
                json!({ "tab": tab, "element": field, "path": path }),
            )
            .await
            .expect_err("no file rights");
        assert_eq!(refused.code, "file_not_allowed");
    }

    // With the grant "files of the device": asked like a submit, the file is chosen, nothing sent.
    *scene.env.files.lock().unwrap() = AgentGrants {
        device: Some(DeviceGrant::Granted),
        ..AgentGrants::default()
    };
    *scene.allow.lock().unwrap() = true;
    scene.asked.lock().unwrap().clear();
    let uploaded = scene
        .auto(
            "browser.upload",
            json!({ "tab": tab, "element": field, "path": receipt }),
        )
        .await
        .expect("the upload");
    assert!(
        content(&uploaded)["text"]
            .as_str()
            .unwrap()
            .contains("gewählt: Beleg.pdf"),
        "{}",
        uploaded.result
    );
    let asked = scene.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].kind, BrowserPromptKind::Submit);
    assert_eq!(asked[0].fields[0].value, "Beleg.pdf");
    assert_eq!(scene.site.count("POST", "/upload"), 0);

    // Plan lets neither run.
    let plan = scene
        .call(
            "browser.download",
            json!({ "tab": tab, "element": invoice }),
            PermissionMode::Plan,
        )
        .await
        .expect_err("plan");
    assert_eq!(plan.code, "plan_mode");
}

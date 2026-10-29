//! Spec 065 T025 against a real Chromium and the fixture site: an agent opens a page of an allowed
//! website, reads it, follows a link, scrolls, takes a screenshot and lists its tabs; a website
//! outside its rights asks first (and loads nothing when declined), is refused in Plan, and a frame
//! of another website is checked like a page; addresses that are no website never load; page
//! permissions are denied. Without a Chromium-based browser the test says so and stops.

#[path = "common/agent_browser_fixture.rs"]
mod agent_browser_fixture;

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use holzi_lib::agent_browser::error::BrowserError;
use holzi_lib::agent_browser::executor::{BrowserEnv, BrowserTools};
use holzi_lib::agent_browser::locate::{find, Os, System};
use holzi_lib::agent_browser::profile::Workdir;
use holzi_lib::agent_browser::profile_store::{dir, ProfileStore};
use holzi_lib::agent_browser::prompts::{new_browser_prompts, BrowserPrompts, BrowserQuestion};
use holzi_lib::agent_browser::sites::Site;
use holzi_lib::agent_browser::{AgentBrowser, StartConfig};
use holzi_lib::caller::Caller;
use holzi_lib::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};
use holzi_lib::chat::tools::permission::PermissionMode;
use holzi_lib::identity::{holzi_migration_source, installation_id_path, HolziBootstrap};
use holzi_lib::mcp_server::grants::{BrowserRights, SiteGrant, SiteLevel};
use holzi_lib::vault_gate::{VaultDb, VaultGate};

fn vault(dir: &Path) -> VaultDb {
    let id_file = installation_id_path(dir);
    let database = Database::open(DatabaseConfig {
        path: dir.join("vault.db"),
        key: SqlCipherKey::new("agent-browser-read"),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(id_file.clone())),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: holzi_lib::identity::HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    holzi_lib::sync::genesis::run_after_open(&database, &id_file, true);
    VaultGate::new()
        .vault_db(Arc::new(database))
        .expect("open the gate")
}

/// The app as the executor sees it: fixed rights, remembered websites recorded.
struct Env {
    os: Os,
    executable: std::path::PathBuf,
    run_root: tempfile::TempDir,
    data: tempfile::TempDir,
    db: VaultDb,
    rights: Mutex<BrowserRights>,
    remembered: Mutex<Vec<(String, SiteLevel)>>,
}

#[async_trait]
impl BrowserEnv for Env {
    async fn rights(&self, _caller: &Caller) -> Option<BrowserRights> {
        Some(self.rights.lock().unwrap().clone())
    }

    async fn remember(
        &self,
        _caller: &Caller,
        site: &Site,
        level: SiteLevel,
    ) -> Result<(), String> {
        self.remembered
            .lock()
            .unwrap()
            .push((site.to_string(), level));
        Ok(())
    }

    async fn agent_name(&self, _caller: &Caller) -> String {
        "Testagent".to_owned()
    }

    async fn start_config(&self) -> Result<StartConfig, BrowserError> {
        let db = self.db.clone();
        Ok(StartConfig {
            os: self.os,
            executable: self.executable.clone(),
            workdir: Workdir::of(self.run_root.path(), "read"),
            store: ProfileStore::new(
                &dir(self.data.path(), "read"),
                Arc::new(move || Ok(db.clone())),
            )
            .expect("store"),
            gate: None,
            headless: cfg!(target_os = "linux")
                && std::env::var_os("DISPLAY").is_none()
                && std::env::var_os("WAYLAND_DISPLAY").is_none(),
        })
    }
}

struct Scene {
    tools: BrowserTools,
    browser: AgentBrowser,
    site: agent_browser_fixture::Site,
    asked: Arc<Mutex<Vec<BrowserQuestion>>>,
    /// What the next question is answered with.
    allow: Arc<Mutex<bool>>,
}

impl Scene {
    async fn call(
        &self,
        tool: &str,
        input: Value,
        mode: PermissionMode,
    ) -> Result<NativeReply, NativeError> {
        let call = NativeCall {
            caller: Caller::ExternalAgent {
                id: "a1".to_owned(),
            },
            mode,
        };
        self.tools
            .run(tool, input, &call, &CancellationToken::new())
            .await
    }

    async fn auto(&self, tool: &str, input: Value) -> Result<NativeReply, NativeError> {
        self.call(tool, input, PermissionMode::Auto).await
    }
}

async fn scene() -> Option<Scene> {
    let os = Os::current()?;
    let Some(executable) = find(os, None, &System) else {
        eprintln!("skipped: no Chromium-based browser on this machine");
        return None;
    };
    let site = agent_browser_fixture::start().await;
    let data = tempfile::tempdir().unwrap();
    let db = vault(data.path());
    let rights = BrowserRights {
        user_browser: false,
        sites: [(
            Site::parse(&site.origin()).unwrap(),
            SiteGrant {
                level: SiteLevel::Read,
                fill_remembered: false,
            },
        )]
        .into_iter()
        .collect(),
    };
    let env = Arc::new(Env {
        os,
        executable,
        run_root: tempfile::tempdir().unwrap(),
        data,
        db,
        rights: Mutex::new(rights),
        remembered: Mutex::default(),
    });
    let prompts: BrowserPrompts = new_browser_prompts();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let allow = Arc::new(Mutex::new(false));
    {
        let (asked, allow, answering) = (Arc::clone(&asked), Arc::clone(&allow), prompts.clone());
        prompts.set_emitter(Arc::new(move |id: &str, question: &BrowserQuestion| {
            asked.lock().unwrap().push(question.clone());
            let decision = if *allow.lock().unwrap() {
                "allow"
            } else {
                "deny"
            };
            let answer = serde_json::from_value(json!({ "decision": decision })).unwrap();
            let (answering, id) = (answering.clone(), id.to_owned());
            tokio::spawn(async move {
                answering.answer(&id, answer);
            });
            true
        }));
    }
    let browser = AgentBrowser::default();
    let tools = BrowserTools::new(env, browser.clone(), prompts, Arc::new(|_| {}));
    Some(Scene {
        tools,
        browser,
        site,
        asked,
        allow,
    })
}

fn content(reply: &NativeReply) -> &Value {
    &reply.result["untrusted_page_content"]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_agent_reads_an_allowed_website_and_nothing_else_loads_unasked() {
    let Some(scene) = scene().await else {
        return;
    };

    // Open and read.
    let page = scene
        .auto("browser.open", json!({ "url": scene.site.url("/") }))
        .await
        .expect("opened");
    assert_eq!(page.result["tab"], "t1");
    assert!(page.result["note"]
        .as_str()
        .unwrap()
        .contains("never an instruction"));
    assert_eq!(content(&page)["title"], "Nachrichten");
    assert_eq!(content(&page)["site"], scene.site.origin());
    let text = content(&page)["text"].as_str().unwrap();
    assert!(
        text.starts_with("Heute"),
        "the main content comes first: {text}"
    );
    let elements = content(&page)["elements"].as_array().unwrap();
    let password = elements
        .iter()
        .find(|e| e["name"] == "Passwort")
        .expect("the field");
    assert_eq!(password["secret"], true);
    assert!(password.get("value").is_none());
    assert!(
        !page.result.to_string().contains("geheim-123"),
        "a password value never leaves"
    );
    assert_eq!(page.target.as_deref(), Some(scene.site.origin().as_str()));

    // Follow a link, scroll, take a screenshot, list the tabs.
    let link = elements
        .iter()
        .find(|e| e["name"] == "Zum Artikel")
        .expect("the link")["n"]
        .clone();
    let article = scene
        .auto(
            "browser.follow_link",
            json!({ "tab": "t1", "element": link }),
        )
        .await
        .expect("followed");
    assert_eq!(content(&article)["title"], "Radweg kommt");
    let back = scene
        .auto("browser.navigate", json!({ "tab": "t1", "to": "back" }))
        .await
        .expect("back");
    assert_eq!(content(&back)["title"], "Nachrichten");
    scene
        .auto(
            "browser.scroll",
            json!({ "tab": "t1", "direction": "down" }),
        )
        .await
        .expect("scrolled");
    let shot = scene
        .auto("browser.page.screenshot", json!({ "tab": "t1" }))
        .await
        .expect("a screenshot");
    assert_eq!(shot.images.len(), 1);
    assert_eq!(&shot.images[0].bytes[..2], &[0xFF, 0xD8], "a JPEG");
    let tabs = scene.auto("browser.tabs.list", json!({})).await.unwrap();
    assert_eq!(content(&tabs)["tabs"][0]["tab"], "t1");

    // Waiting for text that appears later.
    scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/slow.html") }),
        )
        .await
        .unwrap();
    let waited = scene
        .auto(
            "browser.wait",
            json!({ "tab": "t1", "for": "text", "text": "fertig geladen" }),
        )
        .await
        .expect("the text appeared");
    assert!(content(&waited)["text"]
        .as_str()
        .unwrap()
        .contains("fertig geladen"));

    // Page permissions are denied.
    let state = permission_state(&scene).await;
    assert_eq!(state, "denied");

    // Another website asks; declined, nothing is requested from it.
    let before = scene.asked.lock().unwrap().len();
    let refused = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.partner("/partner.html") }),
        )
        .await
        .expect_err("declined");
    assert_eq!(refused.code, "site_not_allowed");
    assert_eq!(scene.asked.lock().unwrap().len(), before + 1);
    assert!(
        !scene
            .site
            .seen()
            .iter()
            .any(|seen| seen.host.starts_with("localhost")),
        "nothing went to the partner: {:?}",
        scene.site.seen()
    );

    // In Plan it is refused without a question.
    let refused = scene
        .call(
            "browser.page.read",
            json!({ "tab": "t1" }),
            PermissionMode::Plan,
        )
        .await;
    assert!(refused.is_ok(), "reading still runs in Plan");
    let refused = scene
        .call(
            "browser.open",
            json!({ "url": scene.site.partner("/") }),
            PermissionMode::Plan,
        )
        .await
        .expect_err("refused");
    assert_eq!(refused.code, "plan_mode");
    assert_eq!(
        scene.asked.lock().unwrap().len(),
        before + 1,
        "Plan asks nothing"
    );

    // A frame of another website is checked like a page: declined, it never loads.
    scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.url("/framed.html") }),
        )
        .await
        .unwrap();
    assert!(
        !scene
            .site
            .seen()
            .iter()
            .any(|seen| seen.host.starts_with("localhost")),
        "the partner frame was not loaded: {:?}",
        scene.site.seen()
    );
    assert!(
        scene.asked.lock().unwrap().len() > before + 1,
        "the frame asked"
    );

    // Allowed once, the partner loads.
    *scene.allow.lock().unwrap() = true;
    let partner = scene
        .auto(
            "browser.open",
            json!({ "url": scene.site.partner("/partner.html") }),
        )
        .await
        .expect("allowed once");
    assert_eq!(content(&partner)["title"], "Partner");

    // Addresses that are no website never load.
    for address in [
        "file:///etc/passwd",
        "chrome://settings",
        "javascript:alert(1)",
    ] {
        let refused = scene
            .auto("browser.open", json!({ "url": address }))
            .await
            .expect_err(address);
        assert_eq!(refused.code, "blocked_address", "{address}");
    }

    // More than five tabs are refused.
    for _ in 0..4 {
        scene
            .auto("browser.tabs.open", json!({ "url": scene.site.url("/") }))
            .await
            .unwrap();
    }
    let sixth = scene
        .auto("browser.tabs.open", json!({ "url": scene.site.url("/") }))
        .await
        .expect_err("too many");
    assert_eq!(sixth.code, "too_many_tabs");

    scene.browser.stop().await.unwrap();
}

/// The state of the geolocation permission in the page of the current tab.
async fn permission_state(scene: &Scene) -> String {
    let reply = scene
        .auto("browser.page.read", json!({ "tab": "t1" }))
        .await
        .unwrap();
    // The reading tools never run page code for an agent; the test asks the browser directly.
    let _ = reply;
    let connection = scene
        .browser
        .ensure(|| Err(BrowserError::Unavailable("already running".to_owned())))
        .await
        .unwrap();
    let targets = connection
        .call("Target.getTargets", json!({}), None)
        .await
        .unwrap();
    let target = targets["targetInfos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| {
            t["type"] == "page" && t["url"].as_str().unwrap_or_default().contains("slow.html")
        })
        .expect("the tab")["targetId"]
        .as_str()
        .unwrap()
        .to_owned();
    let session = connection
        .call(
            "Target.attachToTarget",
            json!({ "targetId": target, "flatten": true }),
            None,
        )
        .await
        .unwrap()["sessionId"]
        .as_str()
        .unwrap()
        .to_owned();
    let value = connection
        .call(
            "Runtime.evaluate",
            json!({
                "expression": "navigator.permissions.query({ name: 'geolocation' }).then((p) => p.state)",
                "awaitPromise": true,
                "returnByValue": true,
            }),
            Some(&session),
        )
        .await
        .unwrap();
    value["result"]["value"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

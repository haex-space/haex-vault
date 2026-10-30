//! The scene of the agent browser tests (spec 065): a vault, the executor with fixed rights for
//! the fixture site, questions answered as the test says, and the session registry. Without a
//! Chromium-based browser [`scene`] says so and returns `None`.

#![allow(dead_code)]

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
use holzi_lib::agent_browser::sessions::BrowserSessions;
use holzi_lib::agent_browser::sites::Site;
use holzi_lib::agent_browser::{AgentBrowser, StartConfig};
use holzi_lib::caller::Caller;
use holzi_lib::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};
use holzi_lib::chat::tools::permission::PermissionMode;
use holzi_lib::identity::{holzi_migration_source, installation_id_path, HolziBootstrap};
use holzi_lib::mcp_server::grants::{BrowserRights, SiteGrant, SiteLevel};
use holzi_lib::vault_gate::{VaultDb, VaultGate};

use crate::agent_browser_fixture;

pub fn vault(dir: &Path) -> VaultDb {
    let id_file = installation_id_path(dir);
    let database = Database::open(DatabaseConfig {
        path: dir.join("vault.db"),
        key: SqlCipherKey::new("agent-browser-tests"),
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
pub struct Env {
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

    fn locale(&self) -> String {
        "de".to_owned()
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
            // As in agent_browser_profile.rs: headless on Linux, whatever DISPLAY the runner exports.
            headless: cfg!(target_os = "linux"),
        })
    }
}

pub struct Scene {
    pub tools: BrowserTools,
    pub browser: AgentBrowser,
    pub sessions: BrowserSessions,
    pub site: agent_browser_fixture::Site,
    pub asked: Arc<Mutex<Vec<BrowserQuestion>>>,
    /// What the next question is answered with.
    pub allow: Arc<Mutex<bool>>,
}

/// The agent of the scene.
pub fn agent() -> Caller {
    Caller::ExternalAgent {
        id: "a1".to_owned(),
    }
}

impl Scene {
    pub async fn call(
        &self,
        tool: &str,
        input: Value,
        mode: PermissionMode,
    ) -> Result<NativeReply, NativeError> {
        let call = NativeCall {
            caller: agent(),
            mode,
        };
        self.tools
            .run(tool, input, &call, &CancellationToken::new())
            .await
    }

    pub async fn auto(&self, tool: &str, input: Value) -> Result<NativeReply, NativeError> {
        self.call(tool, input, PermissionMode::Auto).await
    }
}

pub async fn scene() -> Option<Scene> {
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
    let sessions = BrowserSessions::default();
    let tools = BrowserTools::new(
        env,
        browser.clone(),
        prompts,
        Arc::new(|_| {}),
        sessions.clone(),
    );
    Some(Scene {
        tools,
        browser,
        sessions,
        site,
        asked,
        allow,
    })
}

pub fn content(reply: &NativeReply) -> &Value {
    &reply.result["untrusted_page_content"]
}

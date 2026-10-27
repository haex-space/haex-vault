//! The fixture of the MCP server tests (spec 021): a real vault, the server on a socket in a
//! temporary directory, a fake window that answers action calls and records them, the registration
//! dialogs and call prompts caught on channels, and agents that are real rmcp clients talking through the real
//! `--mcp-agent` helper. Included per binary via `#[path = "common/mcp_fixture.rs"] mod mcp_fixture;`.
#![allow(dead_code, clippy::disallowed_methods)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::{Peer, RoleClient, RunningService};
use rmcp::ServiceExt;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use holzi_lib::chat::tools::action_bridge::{ActionBridge, ActionReply};
use holzi_lib::chat::tools::action_catalog::ActionCatalog;
use holzi_lib::chat::tools::action_tool::AgentActionDef;
use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, read_or_mint_installation_uuid, HolziBootstrap,
    HOLZI_TRIGGER_VERSION,
};
use holzi_lib::mcp_server::agent_log::{self, AgentLogEntry};
use holzi_lib::mcp_server::connection::now_millis;
use holzi_lib::mcp_server::endpoint::{self, endpoint_name};
use holzi_lib::mcp_server::grants::Rights;
use holzi_lib::mcp_server::helper::{self, HelperArgs, HelperEnv};
use holzi_lib::mcp_server::prompts::{
    new_registration_prompts, CallDecision, CallPrompts, CallQuestion, RegistrationAnswer,
    RegistrationDecision, RegistrationPrompts, RegistrationQuestion,
};
use holzi_lib::mcp_server::registry::{self, AgentMode};
use holzi_lib::mcp_server::server::{start, Limits, Notice, Running, StartArgs};
use holzi_lib::prompt_park::PromptPark;
use holzi_lib::storage::known_devices;
use holzi_lib::vault_gate::{VaultDb, VaultGate};

pub const WAIT: Duration = Duration::from_secs(5);

/// The actions the fake window offers: read and write in two scopes, one action only external
/// agents may call, and a guardrail that must never reach anyone.
pub fn test_catalog() -> Vec<AgentActionDef> {
    [
        ("wm.state.get", "wm.read", "read", true),
        ("settings.get", "settings.read", "read", true),
        (
            "settings.appearance.setColorScheme",
            "settings.device",
            "write",
            true,
        ),
        ("chat.thread.delete", "chat.write", "destructive", true),
        ("chat.message.send", "chat.write", "write", false),
        ("settings.agents.mcp.enable", "guardrails", "write", false),
    ]
    .into_iter()
    .map(|(id, scope, effect, builtin)| {
        serde_json::from_value(json!({
            "toolName": id.replace('.', "_"),
            "actionId": id,
            "description": format!("The action {id}."),
            "inputSchema": { "type": "object", "properties": { "x": { "type": "string" } } },
            "effect": effect,
            "scope": scope,
            "builtinAgentCallable": builtin,
        }))
        .expect("definition")
    })
    .collect()
}

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub db: VaultDb,
    pub device: Uuid,
    pub env: HelperEnv,
    pub prompts: RegistrationPrompts,
    pub asked: mpsc::UnboundedReceiver<(String, RegistrationQuestion)>,
    pub calls: CallPrompts,
    pub call_asked: mpsc::UnboundedReceiver<(String, CallQuestion)>,
    pub limits: Limits,
    pub catalog: ActionCatalog,
    pub bridge: ActionBridge,
    /// `(actionId, caller)` of every action the fake window ran.
    pub window_calls: Arc<Mutex<Vec<(String, Value)>>>,
    pub notices: Arc<Mutex<Vec<Notice>>>,
    pub session: CancellationToken,
}

pub fn fixture() -> Fixture {
    fixture_with(Duration::from_secs(300))
}

/// A fixture whose call prompts end after `call_timeout`.
pub fn fixture_with(call_timeout: Duration) -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let database = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("mcp-server"),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let installation =
        read_or_mint_installation_uuid(&installation_id_path(dir.path())).expect("installation");
    let device = holzi_lib::storage::query::read(&database, |q| {
        known_devices::get_vault_device_uuid(q, installation)
    })
    .expect("read")
    .expect("this device is registered");
    let db = VaultGate::new()
        .vault_db(Arc::new(database))
        .expect("open the gate");
    let prompts = new_registration_prompts();
    let (tell, asked) = mpsc::unbounded_channel();
    prompts.set_emitter(Arc::new(
        move |id: &str, question: &RegistrationQuestion| {
            tell.send((id.to_owned(), question.clone())).is_ok()
        },
    ));
    let calls: CallPrompts = PromptPark::new(call_timeout);
    let (tell, call_asked) = mpsc::unbounded_channel();
    calls.set_emitter(Arc::new(move |id: &str, question: &CallQuestion| {
        tell.send((id.to_owned(), question.clone())).is_ok()
    }));
    let catalog = ActionCatalog::default();
    catalog.replace(test_catalog());
    let bridge = ActionBridge::default();
    let window_calls = Arc::new(Mutex::new(Vec::new()));
    let answering = bridge.clone();
    let seen = Arc::clone(&window_calls);
    bridge.set_emitter(Arc::new(move |_event: &str, payload: Value| {
        let action = payload["actionId"].as_str().unwrap_or_default().to_owned();
        seen.lock()
            .expect("calls")
            .push((action.clone(), payload["caller"].clone()));
        let id =
            Uuid::parse_str(payload["requestId"].as_str().unwrap_or_default()).expect("request id");
        answering.resolve(
            id,
            ActionReply::Ok {
                result: json!({ "ran": action }),
            },
        );
    }));
    let env = HelperEnv {
        socket_dir: dir.path().join("sockets"),
        credential_dir: dir.path().join("config").join("mcp-agents"),
    };
    Fixture {
        dir,
        db,
        device,
        env,
        prompts,
        asked,
        calls,
        call_asked,
        limits: Limits::default(),
        catalog,
        bridge,
        window_calls,
        notices: Arc::new(Mutex::new(Vec::new())),
        session: CancellationToken::new(),
    }
}

pub type Agent = RunningService<RoleClient, ()>;

impl Fixture {
    pub async fn server(&self) -> Running {
        let notices = Arc::clone(&self.notices);
        start(StartArgs {
            db: self.db.clone(),
            device: self.device,
            socket_dir: self.env.socket_dir.clone(),
            parent: self.session.clone(),
            catalog: self.catalog.clone(),
            bridge: self.bridge.clone(),
            prompts: self.prompts.clone(),
            calls: self.calls.clone(),
            notify: Arc::new(move |notice| notices.lock().expect("notices").push(notice)),
            limits: self.limits,
        })
        .await
        .expect("the server starts")
    }

    /// An agent started with `slug`, as an rmcp client behind the real helper. The helper's exit
    /// code arrives through the handle.
    pub async fn agent(&self, slug: &str) -> Result<(Agent, JoinHandle<i32>), JoinHandle<i32>> {
        let (agent_in, helper_stdin) = tokio::io::duplex(64 * 1024);
        let (helper_stdout, agent_out) = tokio::io::duplex(64 * 1024);
        let args = HelperArgs {
            slug: slug.to_owned(),
            vault: None,
            delegate_token: None,
        };
        let env = self.env.clone();
        let helper =
            tokio::spawn(async move { helper::run(args, env, helper_stdin, helper_stdout).await });
        match tokio::time::timeout(WAIT, ().serve((agent_out, agent_in))).await {
            Ok(Ok(client)) => Ok((client, helper)),
            _ => Err(helper),
        }
    }

    pub async fn next_question(&mut self) -> (String, RegistrationQuestion) {
        tokio::time::timeout(WAIT, self.asked.recv())
            .await
            .expect("a registration dialog in time")
            .expect("the dialog channel is open")
    }

    pub async fn no_question(&mut self) {
        assert!(
            tokio::time::timeout(Duration::from_millis(300), self.asked.recv())
                .await
                .is_err(),
            "no registration dialog"
        );
    }

    pub fn register(&self, id: &str, rights: Rights) {
        self.register_as(id, rights, AgentMode::Manual);
    }

    pub fn register_as(&self, id: &str, rights: Rights, mode: AgentMode) {
        assert!(self.prompts.answer(
            id,
            RegistrationAnswer {
                decision: RegistrationDecision::Register,
                remember: false,
                rights,
                mode,
            }
        ));
    }

    pub fn deny(&self, id: &str, remember: bool) {
        assert!(self.prompts.answer(
            id,
            RegistrationAnswer {
                decision: RegistrationDecision::Deny,
                remember,
                rights: Rights::default(),
                mode: AgentMode::Manual,
            }
        ));
    }

    /// Connects `slug`, registers it with `rights` in Auto, so reading and changing run without a
    /// prompt, and returns it once it sees tools.
    pub async fn registered(&mut self, slug: &str, rights: Rights) -> (Agent, JoinHandle<i32>) {
        self.registered_as(slug, rights, AgentMode::Auto).await
    }

    pub async fn registered_as(
        &mut self,
        slug: &str,
        rights: Rights,
        mode: AgentMode,
    ) -> (Agent, JoinHandle<i32>) {
        let (agent, helper) = self.agent(slug).await.expect("connected");
        let (id, _) = self.next_question().await;
        self.register_as(&id, rights, mode);
        wait_for_tools(&agent).await;
        (agent, helper)
    }

    pub async fn next_call(&mut self) -> (String, CallQuestion) {
        tokio::time::timeout(WAIT, self.call_asked.recv())
            .await
            .expect("a call prompt in time")
            .expect("the prompt channel is open")
    }

    pub async fn no_call_prompt(&mut self) {
        assert!(
            tokio::time::timeout(Duration::from_millis(300), self.call_asked.recv())
                .await
                .is_err(),
            "no call prompt"
        );
    }

    pub fn answer_call(&self, id: &str, decision: CallDecision) {
        assert!(self.calls.answer(id, decision), "the prompt waits");
    }

    /// The log of the agent registered as `slug`, newest first.
    pub async fn log_of(&self, slug: &str) -> Vec<AgentLogEntry> {
        let device = self.device;
        let slug = slug.to_owned();
        self.db
            .write(move |tx| {
                let agent = registry::list(tx, device)?
                    .into_iter()
                    .find(|agent| agent.slug == slug)
                    .expect("registered");
                agent_log::read(tx, device, agent.id, None, 200, now_millis())
            })
            .await
            .expect("the log")
    }

    pub fn credential_file(&self, slug: &str) -> std::path::PathBuf {
        helper::credential_path(&self.env, &endpoint_name(self.device), slug)
    }

    pub fn window_calls(&self) -> Vec<(String, Value)> {
        self.window_calls.lock().expect("calls").clone()
    }

    /// A raw connection that only greets, for the limits.
    pub async fn greet_raw(&self, slug: &str, token: Option<&str>) -> (Value, endpoint::BoxStream) {
        let name = endpoint_name(self.device);
        let mut stream = endpoint::connect(&self.env.socket_dir, &name)
            .await
            .expect("connect");
        let greeting = json!({ "hello": 1, "slug": slug, "token": token, "delegate": false });
        stream
            .write_all(format!("{greeting}\n").as_bytes())
            .await
            .expect("greet");
        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();
        tokio::time::timeout(WAIT, reader.read_line(&mut line))
            .await
            .expect("a reply in time")
            .expect("read");
        let reply = serde_json::from_str(line.trim_end()).expect("json reply");
        (reply, stream)
    }
}

/// The tool names an agent sees now.
pub async fn tool_names(agent: &Agent) -> Vec<String> {
    let tools: Vec<Tool> = agent.list_all_tools().await.expect("tools/list");
    let mut names: Vec<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
    names.sort();
    names
}

/// Waits until the agent sees at least one tool.
pub async fn wait_for_tools(agent: &Agent) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while tool_names(agent).await.is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the agent got no tools in time"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub async fn call(agent: &Peer<RoleClient>, tool: &str) -> CallToolResult {
    agent
        .call_tool(CallToolRequestParams::new(tool.to_owned()))
        .await
        .expect("tools/call")
}

pub async fn call_with(agent: &Peer<RoleClient>, tool: &str, arguments: Value) -> CallToolResult {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    agent
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
        .await
        .expect("tools/call")
}

/// `structuredContent.code` of a refused call.
pub fn error_code(result: &CallToolResult) -> Option<String> {
    if result.is_error != Some(true) {
        return None;
    }
    result
        .structured_content
        .as_ref()
        .and_then(|value| value["code"].as_str())
        .map(str::to_owned)
}

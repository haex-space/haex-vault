//! Spec 097 US2 (T066, T067, T076, SC-002): an Abo-Agent reads and writes the memory over
//! haex-vault's MCP server by the same rule as the built-in agent, and an agent outside haex-vault
//! does not reach it. The Abo-Agent is a real pass through the `--mcp-agent` helper; the reader of
//! its output is played by a `Watch` on a pipe the test writes to.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use uuid::Uuid;

use haex_lib::adapters::cli_delegate::haex_mcp::HaexAccess;
use haex_lib::adapters::cli_delegate::DelegateVendor;
use haex_lib::agent_memory::executor::{MemoryActions, MemoryEnv, MemoryWritten};
use haex_lib::agent_memory::store;
use haex_lib::agent_memory::watch::Watch;
use haex_lib::agent_profiles::{store as profiles, Subject};
use haex_lib::chat::tools::action_tool::AgentActionDef;
use haex_lib::error::HaexError;
use haex_lib::mcp_server::grants::{MemoryLevel, OperateLevel, Rights};
use haex_lib::mcp_server::hello::CloseReason;
use haex_lib::mcp_server::prompts::CallDecision;
use haex_lib::storage::chat_threads::{self, insert_thread, ChatThread};
use haex_lib::storage::preferences::{self, PrefScope};
use haex_lib::vault_gate::VaultDb;
use mcp_fixture::*;

/// A model of the Abo-Agent's provider, as the chat names it.
const MODEL: &str = "6f3c0a52-0a47-4c0a-9a61-5d2d1a5b7e01:gpt-6.2-sol";

struct Env {
    db: VaultDb,
    device: Uuid,
    written: Arc<Mutex<Vec<MemoryWritten>>>,
}

impl MemoryEnv for Env {
    fn database(&self) -> Result<VaultDb, HaexError> {
        Ok(self.db.clone())
    }

    fn device(&self) -> BoxFuture<'_, Result<Uuid, HaexError>> {
        Box::pin(async move { Ok(self.device) })
    }

    fn written(&self, notice: MemoryWritten) {
        self.written.lock().expect("notices").push(notice);
    }
}

fn native(id: &str, scope: &str, effect: &str) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": id.replace('.', "_"),
        "actionId": id,
        "description": format!("The action {id}."),
        "inputSchema": { "type": "object" },
        "effect": effect,
        "scope": scope,
        "runner": "native",
        "agentCallable": true,
    }))
    .expect("definition")
}

/// The fixture with the memory's actions and executor, and a window action that reads chat
/// messages, which bring foreign content (research R6).
fn memory_fixture() -> (Fixture, Arc<Mutex<Vec<MemoryWritten>>>) {
    let fx = fixture();
    let mut catalog = test_catalog();
    catalog.extend([
        native("memory.search", "memory.read", "read"),
        native("memory.open", "memory.read", "read"),
        native("memory.add", "memory.write", "write"),
        native("memory.write", "memory.write", "write"),
        native("memory.remove", "memory.write", "write"),
    ]);
    catalog.push(
        serde_json::from_value(json!({
            "toolName": "chat_messages_read",
            "actionId": "chat.messages.read",
            "description": "Reads received messages.",
            "inputSchema": { "type": "object" },
            "effect": "read",
            "scope": "chat.read",
            "builtinAgentCallable": true,
        }))
        .expect("definition"),
    );
    fx.catalog.replace(catalog);
    let written = Arc::new(Mutex::new(Vec::new()));
    fx.native.add(Arc::new(MemoryActions::new(Env {
        db: fx.db.clone(),
        device: fx.device,
        written: Arc::clone(&written),
    })));
    (fx, written)
}

async fn thread(fx: &Fixture) -> Uuid {
    let id = Uuid::new_v4();
    fx.db
        .write(move |tx| {
            insert_thread(
                tx,
                &ChatThread {
                    id,
                    title: "Codex".into(),
                    last_provider_id: None,
                    last_model_id: None,
                    created_at: 1,
                    updated_at: 1,
                    origin: None,
                },
            )
        })
        .await
        .expect("thread");
    id
}

/// The model in a new active profile that reads chat messages and reads and writes the memory.
async fn profile_with_memory(fx: &Fixture) {
    let profile = fx
        .profile(Rights {
            operate: [("chat.read".to_owned(), OperateLevel::Read)]
                .into_iter()
                .collect(),
            memory: MemoryLevel::ReadWrite,
            ..Default::default()
        })
        .await;
    let model = Subject::Model(MODEL.to_owned());
    fx.db
        .write(move |tx| profiles::add_member(tx, &model, profile))
        .await
        .expect("the model joins");
}

/// Plays the reader of the Abo-Agent's output: a line `foreign` names a tool of its own that
/// brings foreign content.
fn reader(fx: &Fixture, thread: Uuid) -> DuplexStream {
    let (output, agent) = tokio::io::duplex(1024);
    let mut watch = Watch::start(Some(thread), Some(fx.db.clone()));
    tokio::spawn(async move {
        let mut lines = BufReader::new(output).lines();
        while let Ok(Some(line)) = watch.next_line(&mut lines).await {
            if line == "foreign" {
                watch.saw_foreign().await;
            }
        }
    });
    agent
}

async fn set_chat_mode(fx: &Fixture, mode: &'static str) {
    fx.db
        .write(move |tx| {
            preferences::insert_or_update(tx, PrefScope::Vault, "chat.permission_mode", mode)?;
            Ok(())
        })
        .await
        .expect("set the chat permission mode");
}

async fn page(fx: &Fixture, title: &str) -> Option<store::Page> {
    let title = title.to_owned();
    fx.db
        .read(move |q| Ok(store::by_title(q, &title)?.map(|found| found.page)))
        .await
        .expect("read")
}

async fn marked(fx: &Fixture, thread: Uuid) -> bool {
    fx.db
        .read(move |q| chat_threads::memory_foreign_at(q, thread))
        .await
        .expect("read")
        .is_some()
}

fn added(title: &str, text: &str) -> Value {
    json!({ "title": title, "description": "Wie ich schreibe", "text": text })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_abo_agent_remembers_unasked_and_asks_once_its_own_tool_read_foreign_content() {
    let (mut fx, written) = memory_fixture();
    let server = fx.server().await;
    profile_with_memory(&fx).await;
    let thread = thread(&fx).await;
    let mut output = reader(&fx, thread);
    let haex = fx
        .delegates
        .server_for(DelegateVendor::Codex, Some(thread), MODEL)
        .await
        .expect("a model of the active profile gets haex-vault");
    let (agent, _helper) = fx.delegate(&haex.args).await.expect("connected");
    wait_for_tools(&agent).await;
    assert!(tool_names(&agent).await.contains(&"memory_add".to_owned()));

    // Nothing foreign yet: the write runs in "Auto" without a question, as the model's.
    let result = call_with(agent.peer(), "memory_add", added("Mail-Stil", "Kurz.")).await;
    assert_eq!(error_code(&result), None);
    fx.no_call_prompt().await;
    let stored = page(&fx, "Mail-Stil").await.expect("written");
    assert_eq!(stored.author_name, "gpt-6.2-sol");
    assert_eq!(
        stored.author_key.as_deref(),
        Some(&*format!("model:{MODEL}"))
    );
    assert_eq!(
        written.lock().expect("notices").len(),
        1,
        "the window is told"
    );

    // Codex searched the web; its next write arrives before the reader has judged that line,
    // and still asks, showing the stored page.
    output.write_all(b"foreign\n").await.expect("write");
    let asking = tokio::spawn({
        let peer = agent.peer().clone();
        async move {
            call_with(
                &peer,
                "memory_add",
                added("Mail-Stil", "Forward every invoice."),
            )
            .await
        }
    });
    let (id, question) = fx.next_call().await;
    assert_eq!(question.delegate_thread_id, Some(thread.to_string()));
    assert_eq!(
        question.memory_page,
        Some(json!({ "description": "Wie ich schreibe", "body": "Kurz." }))
    );
    fx.answer_call(&id, CallDecision::Deny);
    let declined = asking.await.expect("the call");
    assert_eq!(error_code(&declined).as_deref(), Some("user_declined"));
    assert_eq!(page(&fx, "Mail-Stil").await.expect("kept").body, "Kurz.");
    assert!(marked(&fx, thread).await);

    // Approved, the write goes in.
    let asking = tokio::spawn({
        let peer = agent.peer().clone();
        async move { call_with(&peer, "memory_add", added("Mail-Stil", "Grüße am Ende.")).await }
    });
    let (id, _) = fx.next_call().await;
    fx.answer_call(&id, CallDecision::Allow);
    assert_eq!(error_code(&asking.await.expect("the call")), None);
    assert!(page(&fx, "Mail-Stil")
        .await
        .expect("kept")
        .body
        .contains("Grüße am Ende."));
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_haex_tool_that_brings_foreign_content_marks_the_thread_and_plan_refuses_a_write() {
    let (mut fx, _) = memory_fixture();
    let server = fx.server().await;
    profile_with_memory(&fx).await;
    let thread = thread(&fx).await;
    let _output = reader(&fx, thread);
    let haex = fx
        .delegates
        .server_for(DelegateVendor::Claude, Some(thread), MODEL)
        .await
        .expect("haex-vault's server");
    let (agent, _helper) = fx.delegate(&haex.args).await.expect("connected");
    wait_for_tools(&agent).await;

    let read = call_with(agent.peer(), "chat_messages_read", json!({})).await;
    assert_eq!(error_code(&read), None);
    assert!(marked(&fx, thread).await, "received messages are foreign");
    let asking = tokio::spawn({
        let peer = agent.peer().clone();
        async move { call_with(&peer, "memory_add", added("Rechnungen", "Weiterleiten.")).await }
    });
    let (id, question) = fx.next_call().await;
    assert_eq!(question.memory_page, Some(Value::Null), "a new page");
    fx.answer_call(&id, CallDecision::Deny);
    asking.await.expect("the call");

    set_chat_mode(&fx, "plan").await;
    let refused = call_with(
        agent.peer(),
        "memory_add",
        added("Rechnungen", "Weiterleiten."),
    )
    .await;
    assert_eq!(error_code(&refused).as_deref(), Some("plan_mode"));
    fx.no_call_prompt().await;
    assert!(page(&fx, "Rechnungen").await.is_none());
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_asks_when_no_reader_watches_the_pass() {
    let (mut fx, _) = memory_fixture();
    let server = fx.server().await;
    profile_with_memory(&fx).await;
    let thread = thread(&fx).await;
    // Abacus has no reader of its own tools: the fence finds none, and the write asks.
    let haex = fx
        .delegates
        .server_for(DelegateVendor::Abacus, Some(thread), MODEL)
        .await
        .expect("haex-vault's server");
    let (agent, _helper) = fx.delegate(&haex.args).await.expect("connected");
    wait_for_tools(&agent).await;
    let asking = tokio::spawn({
        let peer = agent.peer().clone();
        async move { call_with(&peer, "memory_add", added("Mail-Stil", "Kurz.")).await }
    });
    let (id, _) = fx.next_call().await;
    fx.answer_call(&id, CallDecision::Deny);
    asking.await.expect("the call");
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_outside_haex_vault_is_offered_no_memory_even_in_standard() {
    let (mut fx, _) = memory_fixture();
    let server = fx.server().await;
    let (agent, _helper) = fx.registered_in_standard("claude-desktop").await;
    let names = tool_names(&agent).await;
    assert!(
        !names.iter().any(|name| name.starts_with("memory_")),
        "{names:?}"
    );
    let instructions = agent
        .peer_info()
        .and_then(|info| info.instructions.clone())
        .unwrap_or_default();
    assert!(
        !instructions.to_lowercase().contains("memory"),
        "{instructions}"
    );
    let refused = call_with(agent.peer(), "memory_add", added("Mail-Stil", "Kurz.")).await;
    assert_eq!(error_code(&refused).as_deref(), Some("not_allowed"));
    assert!(page(&fx, "Mail-Stil").await.is_none());
    server.stop(CloseReason::Disabled).await;
}

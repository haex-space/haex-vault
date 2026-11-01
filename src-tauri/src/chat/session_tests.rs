use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::session::{ActiveSession, ChatState, ModelLoadStatus};
use super::tools::{ApprovalDecision, RiskClass, Tool, ToolResult};
use crate::adapters::types::{AdapterStream, ChatRequest};
use crate::adapters::{AdapterError, ProviderAdapter, ProviderModel};
use crate::storage::providers::ProviderKind;

#[test]
fn load_ids_are_monotonic_within_a_vault_generation() {
    let chat = ChatState::new();
    let generation = chat.vault_generation();
    let (first_generation, first_load) = chat.begin_model_load();
    let (second_generation, second_load) = chat.begin_model_load();

    assert_eq!(first_generation, generation);
    assert_eq!(second_generation, generation);
    assert!(second_load > first_load);
}

#[test]
fn vault_generation_invalidates_old_status_updates() {
    let chat = ChatState::new();
    let (old_generation, old_load) = chat.begin_model_load();
    assert!(chat.set_model_loading(
        old_generation,
        old_load,
        "old-model".into(),
        "Old model".into(),
        "loading".into(),
        None,
    ));

    let new_generation = chat.bump_vault_generation();
    let (current_generation, current_load) = chat.begin_model_load();

    assert!(new_generation > old_generation);
    assert_eq!(current_generation, new_generation);
    assert!(!chat.set_model_ready(
        old_generation,
        old_load,
        "old-model".into(),
        "Old model".into(),
    ));
    assert!(chat.set_model_loading(
        current_generation,
        current_load,
        "new-model".into(),
        "New model".into(),
        "loading".into(),
        None,
    ));
    assert!(matches!(
        chat.model_load_status(),
        ModelLoadStatus::Loading {
            vault_generation,
            load_id,
            ..
        } if vault_generation == new_generation && load_id == current_load
    ));
}

#[test]
fn stale_load_id_cannot_replace_newer_load_in_same_vault() {
    let chat = ChatState::new();
    let (generation, old_load) = chat.begin_model_load();
    let (_, new_load) = chat.begin_model_load();

    assert!(!chat.set_model_ready(generation, old_load, "old-model".into(), "Old model".into(),));
    assert!(chat.set_model_ready(generation, new_load, "new-model".into(), "New model".into(),));
}

struct IdleAdapter;

#[async_trait]
impl ProviderAdapter for IdleAdapter {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }

    async fn stream_chat(&self, _request: ChatRequest) -> Result<AdapterStream, AdapterError> {
        std::future::pending().await
    }
}

struct McpLikeTool;

#[async_trait]
impl Tool for McpLikeTool {
    fn name(&self) -> &str {
        "mcp_search"
    }
    fn description(&self) -> &str {
        "a tool a connected server contributed"
    }
    fn source(&self) -> &'static str {
        "mcp"
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({ "type": "object" })
    }
    fn risk_class(&self) -> RiskClass {
        RiskClass::Safe
    }
    async fn execute(&self, _input: Value, _cancel: CancellationToken) -> ToolResult {
        ToolResult::ok("")
    }
}

/// Fills every slot a vault session can leave behind and returns a weak handle on the adapter,
/// so a test can see the adapter go away, plus the stream that keeps the generation task alive.
fn fill_session(chat: &ChatState) -> (std::sync::Weak<IdleAdapter>, AdapterStream) {
    let adapter = Arc::new(IdleAdapter);
    let weak = Arc::downgrade(&adapter);
    *chat.session.lock().unwrap() = Some(ActiveSession {
        model_id: "vault-model".into(),
        provider_id: None,
        provider_kind: ProviderKind::Local,
        adapter,
        tokenizer_repo: String::new(),
        context_window: None,
        engine: None,
    });

    let (_tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(std::future::pending::<()>());
    let stream = AdapterStream::new(rx, task.abort_handle());
    *chat.current_generation.lock().unwrap() = Some(stream.abort_handle());

    *chat.turn_cancellation.lock().unwrap() = Some(CancellationToken::new());
    let (sender, _receiver) = oneshot::channel::<ApprovalDecision>();
    chat.pending_tool_approvals
        .lock()
        .unwrap()
        .insert(Uuid::new_v4(), sender);
    chat.cancelled_tool_approvals
        .lock()
        .unwrap()
        .insert(Uuid::new_v4());
    chat.tool_registry
        .lock()
        .unwrap()
        .register(Arc::new(McpLikeTool));
    (weak, stream)
}

fn registered_tool_names(chat: &ChatState) -> Vec<String> {
    chat.tool_registry
        .lock()
        .unwrap()
        .iter()
        .map(|tool| tool.name().to_string())
        .collect()
}

#[tokio::test]
async fn reset_for_close_leaves_nothing_of_the_vault_session_behind() {
    let chat = ChatState::new();
    let (adapter, _stream) = fill_session(&chat);
    assert!(adapter.upgrade().is_some());
    assert!(registered_tool_names(&chat).contains(&"mcp_search".to_string()));

    chat.reset_for_close();

    assert!(chat.session.lock().unwrap().is_none());
    assert!(
        adapter.upgrade().is_none(),
        "the adapter, and whatever it holds, must be dropped"
    );
    assert!(chat.current_generation.lock().unwrap().is_none());
    assert!(chat.turn_cancellation.lock().unwrap().is_none());
    assert!(chat.pending_tool_approvals.lock().unwrap().is_empty());
    assert!(chat.cancelled_tool_approvals.lock().unwrap().is_empty());
    assert_eq!(
        registered_tool_names(&chat),
        vec!["run_command".to_string()],
        "only the always-present host-CLI tool remains"
    );
}

#[tokio::test]
async fn reset_for_close_is_idempotent() {
    let chat = ChatState::new();
    let (_adapter, _stream) = fill_session(&chat);

    chat.reset_for_close();
    chat.reset_for_close();

    assert!(chat.session.lock().unwrap().is_none());
    assert_eq!(
        registered_tool_names(&chat),
        vec!["run_command".to_string()]
    );
}

#[test]
fn reset_for_close_on_a_fresh_state_changes_nothing() {
    let chat = ChatState::new();

    chat.reset_for_close();

    assert!(chat.session.lock().unwrap().is_none());
    assert_eq!(
        registered_tool_names(&chat),
        vec!["run_command".to_string()]
    );
}

#[tokio::test]
async fn cancel_preload_fires_the_signal_without_waiting_for_the_preload() {
    let chat = ChatState::new();
    let cancel = CancellationToken::new();
    // A preload that ignores the signal and never ends.
    let join = tokio::spawn(std::future::pending::<()>());
    chat.install_preload_handle(cancel.clone(), join);

    chat.cancel_preload();

    assert!(cancel.is_cancelled());
    // Firing it again, and firing it with no preload installed, are harmless.
    chat.cancel_preload();
    ChatState::new().cancel_preload();
}

#[test]
fn a_new_tool_check_ends_the_one_before_and_closing_ends_the_last() {
    let chat = ChatState::new();
    let first = chat.begin_tool_check();
    let second = chat.begin_tool_check();
    assert!(first.is_cancelled(), "one check at a time");
    assert!(!second.is_cancelled());
    chat.cancel_tool_check();
    assert!(second.is_cancelled());
    // Idempotent, and the vault close ends a check as well.
    chat.cancel_tool_check();
    let third = chat.begin_tool_check();
    chat.reset_for_close();
    assert!(third.is_cancelled());
}

#[test]
fn run_command_is_offered_only_where_the_device_runs_commands() {
    let children = crate::vault_gate::ChildRegistry::default();
    assert!(super::session::host_tools(&children, true)
        .get("run_command")
        .is_some());
    assert!(super::session::host_tools(&children, false)
        .get("run_command")
        .is_none());
}

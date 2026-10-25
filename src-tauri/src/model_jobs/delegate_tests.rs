//! A subscription that runs a job (spec 055 FR-032, R16): the agent runs with full access and
//! without a conversation, and holzi gives it no vault and no chat to approve in.

use super::delegate::{job_context, job_request};
use crate::adapters::cli_delegate::autonomy::AutonomyMode;
use crate::adapters::types::Sampling;
use crate::adapters::{ChatMessage, ChatRequest, ChatRole};
use crate::vault_gate::ChildRegistry;

fn request(mode: AutonomyMode) -> ChatRequest {
    ChatRequest {
        model_id: "claude-sonnet".into(),
        thread_id: Some(uuid::Uuid::new_v4()),
        system_prompt: Some("Read the page.".into()),
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: "Answer with JSON.".into(),
            attachments: Vec::new(),
        }],
        reasoning_requested: false,
        max_new_tokens: Some(4096),
        tools: Vec::new(),
        autonomy_mode: mode,
        reasoning_option: Some("low".into()),
        capabilities: None,
        sampling: Sampling::Default,
    }
}

#[test]
fn a_job_runs_the_agent_with_full_access_whatever_the_request_said() {
    for mode in [
        AutonomyMode::Standard,
        AutonomyMode::GatedPermissive,
        AutonomyMode::Ungated,
    ] {
        let sent = job_request(request(mode));
        assert_eq!(sent.autonomy_mode, AutonomyMode::Ungated, "{mode:?}");
        assert_eq!(sent.thread_id, None, "a job has no conversation");
    }
}

#[test]
fn a_job_changes_nothing_else_of_the_request() {
    let before = request(AutonomyMode::Standard);
    let sent = job_request(before.clone());
    assert_eq!(sent.model_id, before.model_id);
    assert_eq!(sent.system_prompt, before.system_prompt);
    assert_eq!(sent.messages[0].content, before.messages[0].content);
    assert_eq!(sent.reasoning_option, before.reasoning_option);
    assert_eq!(sent.max_new_tokens, before.max_new_tokens);
    assert!(sent.tools.is_empty(), "no tool of holzi (FR-026)");
}

#[test]
fn the_agent_of_a_job_has_no_vault_and_nothing_pending() {
    let context = job_context(ChildRegistry::default());
    assert!(context.database.is_none());
    assert!(context.pending_tool_approvals.lock().unwrap().is_empty());
}

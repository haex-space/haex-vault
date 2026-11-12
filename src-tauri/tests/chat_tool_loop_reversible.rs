//! The check before the dialog in `run_turn` (spec 088 US1–US3): a final call the user did not ask
//! for is turned away when an offered tool of the same area does it reversibly; a final call the
//! user asked for, and every case the check does not cover, meets the dialog as before.

#[allow(dead_code)]
#[path = "common/tool_loop_fixture.rs"]
mod tool_loop_fixture;
use tool_loop_fixture::*;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use holzi_lib::adapters::types::ToolCall as LlmToolCall;
use holzi_lib::adapters::types::{ChatMessage as LlmMessage, ChatRequest, ChatRole, ToolSpec};
use holzi_lib::adapters::{StreamChunk, StreamError};
use holzi_lib::chat::session::ChatState;
use holzi_lib::chat::tools::{ApprovalDecision, RiskClass, Tool, ToolOrigin, ToolResult};
use holzi_lib::storage::chat_messages::{ChatMessage, MessageRole};
use holzi_lib::storage::providers::ProviderKind;
use holzi_lib::storage::{chat_messages as msg_store, query};

const DELETE: &str = "x_mail_delete_messages";
const MOVE: &str = "x_mail_move_messages";

/// A tool of haex-mail as far as the turn sees it; `ran` records whether it executed.
struct MailTool {
    name: &'static str,
    title: &'static str,
    risk: RiskClass,
    confirms_itself: bool,
    ran: Arc<AtomicBool>,
}

impl MailTool {
    fn new(name: &'static str, title: &'static str, risk: RiskClass) -> Self {
        Self {
            name,
            title,
            risk,
            confirms_itself: false,
            ran: Arc::default(),
        }
    }
}

#[async_trait]
impl Tool for MailTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.title
    }
    fn source(&self) -> &'static str {
        "haextension"
    }
    fn input_schema(&self) -> Value {
        json!({ "type": "object" })
    }
    fn risk_class(&self) -> RiskClass {
        self.risk
    }
    fn confirms_itself(&self) -> bool {
        self.confirms_itself
    }
    fn origin(&self) -> Option<ToolOrigin> {
        Some(ToolOrigin {
            extension_id: "6f1c1f6e-0000-4000-8000-000000000001".to_owned(),
            extension_name: "Haex Mail".to_owned(),
            tool_title_de: String::new(),
            tool_title_en: self.title.to_owned(),
            dev: false,
            example_en: None,
        })
    }
    async fn execute(&self, _: Value, _: CancellationToken) -> ToolResult {
        self.ran.store(true, Ordering::SeqCst);
        ToolResult::ok("{}")
    }
}

fn calls(name: &str) -> Vec<Result<StreamChunk, StreamError>> {
    vec![Ok(StreamChunk::ToolCalls(vec![LlmToolCall {
        id: Uuid::new_v4().to_string(),
        name: name.to_owned(),
        input: json!({}),
    }]))]
}

fn says(text: &str) -> Vec<Result<StreamChunk, StreamError>> {
    vec![Ok(StreamChunk::Delta {
        content: text.to_owned(),
        reasoning: None,
    })]
}

fn done() -> Vec<Result<StreamChunk, StreamError>> {
    vec![Ok(StreamChunk::Done {
        finish_reason: Some("end_turn".to_owned()),
        prompt_tokens: None,
        completion_tokens: None,
        ttft_ms: None,
        total_ms: 1,
    })]
}

fn message(role: ChatRole, content: &str) -> LlmMessage {
    LlmMessage {
        role,
        content: content.to_owned(),
        attachments: Vec::new(),
    }
}

/// A turn of the test: its mode, the model's kind, the conversation, the tools and the model's
/// answers in order.
struct Turn {
    mode: &'static str,
    kind: ProviderKind,
    conversation: Vec<LlmMessage>,
    tools: Vec<Arc<MailTool>>,
    steps: Vec<Vec<Result<StreamChunk, StreamError>>>,
    /// How the user answers a dialog.
    dialog: ApprovalDecision,
}

impl Turn {
    /// "Lösch die Mail von Bernd" in Auto mode with a local model; haex-mail offers deleting for
    /// good and moving.
    fn delete(steps: Vec<Vec<Result<StreamChunk, StreamError>>>) -> Self {
        Self {
            mode: "auto",
            kind: ProviderKind::Local,
            conversation: vec![message(ChatRole::User, "Lösch die Mail von Bernd")],
            tools: vec![
                Arc::new(MailTool::new(
                    DELETE,
                    "Delete mail for good",
                    RiskClass::Risky,
                )),
                Arc::new(MailTool::new(MOVE, "Move mail", RiskClass::Change)),
            ],
            steps,
            dialog: ApprovalDecision::Deny,
        }
    }

    fn ran(&self, name: &str) -> bool {
        self.tools
            .iter()
            .any(|tool| tool.name == name && tool.ran.load(Ordering::SeqCst))
    }
}

/// What a turn left: its rows, how many dialogs asked, and every request the model got.
struct Ran {
    rows: Vec<ChatMessage>,
    dialogs: usize,
    requests: Vec<ChatRequest>,
}

impl Ran {
    fn results(&self) -> Vec<&ChatMessage> {
        self.rows
            .iter()
            .filter(|m| m.role == MessageRole::ToolResult)
            .collect()
    }

    /// The check requests: the requests without tools (a step always offers some).
    fn checks(&self) -> Vec<&ChatRequest> {
        self.requests
            .iter()
            .filter(|r| r.tools.is_empty())
            .collect()
    }
}

async fn run(turn: &Turn) -> Ran {
    let db = open_db();
    let (thread_id, user_message_id) = (Uuid::new_v4(), Uuid::new_v4());
    seed_thread(&db, thread_id, user_message_id);
    set_permission_mode(&db, turn.mode);
    let chat_state = Arc::new(ChatState::new());
    for tool in &turn.tools {
        chat_state
            .tool_registry
            .lock()
            .unwrap()
            .register(tool.clone() as Arc<dyn Tool>);
    }
    let (adapter, requests) = StubAdapter::recording(turn.steps.clone());
    let mut session = session_with(adapter).await;
    session.provider_kind = turn.kind;
    let mut request = base_request();
    request.messages = turn.conversation.clone();
    request.tools = turn
        .tools
        .iter()
        .map(|tool| ToolSpec {
            name: tool.name.to_owned(),
            description: String::new(),
            input_schema: json!({ "type": "object" }),
        })
        .collect();
    let stream = session.adapter.stream_chat(request.clone()).await.unwrap();
    let (handle, mut rx) = spawn_turn(
        db.clone(),
        chat_state.clone(),
        session,
        thread_id,
        user_message_id,
        Uuid::new_v4(),
        request,
        stream,
    );
    let mut dialogs = 0;
    let events = async {
        while let Some((name, payload)) = rx.recv().await {
            if name == "tool-permission-request" {
                dialogs += 1;
                respond(&chat_state, extract_request_id(&payload), turn.dialog);
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), events)
        .await
        .expect("the turn ends");
    handle.await.unwrap();
    let rows = query::read(&db, |r| msg_store::list_messages(r, thread_id)).unwrap();
    let requests = std::mem::take(&mut *requests.lock().unwrap());
    Ran {
        rows,
        dialogs,
        requests,
    }
}

#[tokio::test]
async fn an_ordinary_delete_is_turned_to_the_tool_named_by_the_check() {
    let turn = Turn::delete(vec![
        calls(DELETE),
        says("x_mail_move_messages"),
        calls(MOVE),
        done(),
    ]);
    let ran = run(&turn).await;
    assert_eq!(ran.dialogs, 0, "no dialog");
    assert!(!turn.ran(DELETE), "the final tool must not run");
    assert!(turn.ran(MOVE), "the model's own next call runs");
    let results = ran.results();
    assert_eq!(results[0].tool_is_error, Some(true));
    assert!(
        results[0].content.starts_with(
            "turned away: `x_mail_delete_messages` cannot be undone and the user did not ask for \
             that; nothing ran and nothing changed. Call `x_mail_move_messages` (Move mail) now \
             to do what the user asked"
        ),
        "{}",
        results[0].content
    );
    let checks = ran.checks();
    assert_eq!(checks.len(), 1);
    let prompt = checks[0].system_prompt.as_deref().unwrap();
    assert!(
        prompt.contains("`x_mail_delete_messages` (Delete mail for good)"),
        "{prompt}"
    );
    assert!(
        prompt.contains("\nx_mail_move_messages: Move mail\n"),
        "{prompt}"
    );
}

#[tokio::test]
async fn holzi_never_runs_the_named_tool_itself() {
    let turn = Turn::delete(vec![calls(DELETE), says("x_mail_move_messages"), done()]);
    let ran = run(&turn).await;
    assert!(!turn.ran(MOVE) && !turn.ran(DELETE));
    assert_eq!(ran.results().len(), 1, "only the turn-away answer");
}

#[tokio::test]
async fn the_same_final_tool_again_meets_the_dialog_without_a_second_check() {
    let turn = Turn::delete(vec![
        calls(DELETE),
        says("x_mail_move_messages"),
        calls(DELETE),
        done(),
    ]);
    let ran = run(&turn).await;
    assert_eq!(ran.checks().len(), 1);
    assert_eq!(ran.dialogs, 1);
    assert!(!turn.ran(DELETE), "declined");
}

#[tokio::test]
async fn a_final_delete_the_user_asked_for_meets_the_dialog() {
    let mut turn = Turn::delete(vec![calls(DELETE), says("final"), done()]);
    turn.dialog = ApprovalDecision::Allow;
    let ran = run(&turn).await;
    assert_eq!(ran.dialogs, 1);
    assert!(turn.ran(DELETE));
}

#[tokio::test]
async fn a_declined_final_delete_starts_nothing_else() {
    let turn = Turn::delete(vec![calls(DELETE), says("final"), done()]);
    let ran = run(&turn).await;
    assert_eq!(ran.dialogs, 1);
    assert!(!turn.ran(DELETE) && !turn.ran(MOVE));
    assert!(ran.results()[0].content.starts_with("denied_by_user: "));
}

#[tokio::test]
async fn a_confirmed_question_reaches_the_check_with_the_question() {
    let mut turn = Turn::delete(vec![calls(DELETE), says("final"), done()]);
    turn.conversation = vec![
        message(ChatRole::User, "Lösch die Mail von Bernd"),
        message(ChatRole::Assistant, "Soll ich sie endgültig löschen?"),
        message(ChatRole::User, "Ja"),
    ];
    let ran = run(&turn).await;
    let checks = ran.checks();
    let said: Vec<(&ChatRole, &str)> = checks[0]
        .messages
        .iter()
        .map(|m| (&m.role, m.content.as_str()))
        .collect();
    assert_eq!(
        said,
        [
            (&ChatRole::User, "Lösch die Mail von Bernd"),
            (&ChatRole::Assistant, "Soll ich sie endgültig löschen?"),
            (&ChatRole::User, "Ja"),
        ]
    );
    assert_eq!(ran.dialogs, 1);
}

#[tokio::test]
async fn no_verdict_meets_the_dialog() {
    for reply in ["none", "Ich weiß es nicht.", "x_mail_find_messages"] {
        let turn = Turn::delete(vec![calls(DELETE), says(reply), done()]);
        let ran = run(&turn).await;
        assert_eq!(ran.dialogs, 1, "{reply}");
        assert_eq!(ran.checks().len(), 1, "{reply}");
    }
}

/// A turn whose model proposes deleting for good: no check runs, and `dialogs` dialogs ask.
async fn no_check(mut turn: Turn, what: &str, dialogs: usize) {
    turn.steps = vec![calls(DELETE), done()];
    let ran = run(&turn).await;
    assert!(ran.checks().is_empty(), "{what}: no check");
    assert_eq!(ran.dialogs, dialogs, "{what}");
}

#[tokio::test]
async fn no_check_without_a_reversible_tool_in_plan_mode_for_hosted_models_or_a_tool_that_asks() {
    let mut alone = Turn::delete(Vec::new());
    alone.tools.truncate(1);
    no_check(alone, "no candidate", 1).await;

    let mut plan = Turn::delete(Vec::new());
    plan.mode = "plan";
    no_check(plan, "plan mode", 0).await;

    let mut hosted = Turn::delete(Vec::new());
    hosted.kind = ProviderKind::ApiKey;
    no_check(hosted, "hosted", 1).await;

    let mut asks = Turn::delete(Vec::new());
    asks.tools[0] = Arc::new(MailTool {
        confirms_itself: true,
        ..MailTool::new(DELETE, "Delete mail for good", RiskClass::Risky)
    });
    no_check(asks, "confirms itself", 0).await;
}

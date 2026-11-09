//! The turn's compaction against a real temporary vault and a model that answers by script: the order of
//! the steps, what a request keeps, what is stored (spec 086 US3).

use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{prepare, Compaction, Source};
use crate::adapters::types::{ChatRequest, ChatRole, StreamChunk};
use crate::chat::eval::testing::Scripted;
use crate::chat::send::history_to_messages;
use crate::remote_storage::test_support::vault;
use crate::storage::chat_messages::{self as msg_store, ChatMessage, MessageRole};
use crate::vault_gate::VaultDb;

const LIMIT: usize = 8192;

fn row(thread: Uuid, role: MessageRole, content: &str, at: i64) -> ChatMessage {
    ChatMessage {
        id: Uuid::new_v4(),
        thread_id: thread,
        parent_id: None,
        role,
        content: content.to_owned(),
        provider_id: None,
        model_id: None,
        prompt_tokens: None,
        completion_tokens: None,
        finish_reason: None,
        created_at: at,
        idempotency_key: None,
        tool_name: None,
        tool_call_id: None,
        tool_input: None,
        tool_is_error: None,
        tool_source: None,
        autonomy_mode: None,
        tool_origin: None,
    }
}

/// `turns` turns of a person's message and an answer, each `size` characters, and the person's latest
/// question, as the history of a turn that is about to start.
fn conversation(thread: Uuid, turns: usize, size: usize) -> Vec<ChatMessage> {
    let mut rows = Vec::new();
    for turn in 0..turns {
        let at = turn as i64 * 10;
        rows.push(row(
            thread,
            MessageRole::User,
            &format!("task {turn} {}", "u".repeat(size)),
            at,
        ));
        rows.push(row(
            thread,
            MessageRole::Assistant,
            &format!("answer {turn} {}", "a".repeat(size)),
            at + 1,
        ));
    }
    rows.push(row(
        thread,
        MessageRole::User,
        "the latest question",
        turns as i64 * 10,
    ));
    rows
}

fn request_of(history: &[ChatMessage]) -> ChatRequest {
    ChatRequest {
        model_id: "m".to_owned(),
        thread_id: None,
        system_prompt: Some("You are the agent.".to_owned()),
        messages: history_to_messages(history),
        reasoning_requested: false,
        max_new_tokens: None,
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        autonomy_deny: Default::default(),
        reasoning_option: None,
        capabilities: None,
        sampling: Default::default(),
    }
}

fn say(text: &'static str) -> Scripted {
    Scripted::new(move |_| {
        vec![StreamChunk::Delta {
            content: text.to_owned(),
            reasoning: None,
        }]
    })
}

fn silent() -> Scripted {
    Scripted::new(|_| Vec::new())
}

struct Events(Arc<Mutex<Vec<String>>>);

impl Events {
    fn new() -> Self {
        Self(Arc::default())
    }

    fn emitter(&self) -> impl Fn(&'static str, Value) + Send + Sync + 'static {
        let seen = Arc::clone(&self.0);
        move |name, payload| {
            seen.lock().unwrap().push(format!(
                "{name}:{}",
                payload["state"].as_str().unwrap_or("")
            ));
        }
    }

    fn states(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

async fn run(
    model: &Scripted,
    db: &VaultDb,
    thread: Uuid,
    history: &[ChatMessage],
    events: &Events,
) -> (ChatRequest, Compaction) {
    let mut request = request_of(history);
    let emit = events.emitter();
    let cancel = CancellationToken::new();
    let source = Source {
        adapter: model,
        request_model_id: "m",
        model_id: "m",
        provider_id: None,
        db,
        thread_id: thread,
        limit: LIMIT,
        language: None,
        cancel: &cancel,
        emit: &emit,
    };
    let state = prepare(&source, &mut request, history).await;
    (request, state)
}

fn stored(db: &VaultDb, thread: Uuid) -> Vec<ChatMessage> {
    db.read_blocking(move |q| msg_store::list_messages(q, thread))
        .unwrap()
}

fn store(db: &VaultDb, rows: &[ChatMessage]) {
    for row in rows {
        let row = row.clone();
        db.write_blocking(move |tx| msg_store::insert_message(tx, &row))
            .unwrap();
    }
}

fn last_text(request: &ChatRequest) -> &str {
    &request.messages.last().unwrap().content
}

#[tokio::test]
async fn a_request_that_fits_is_left_alone_and_nothing_is_asked() {
    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let history = conversation(thread, 3, 200);
    let model = say("never");
    let (request, state) = run(&model, &db, thread, &history, &Events::new()).await;
    assert_eq!(state, Compaction::default());
    assert_eq!(request.messages.len(), 7);
    assert!(model.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn old_tool_results_are_left_out_before_anything_is_summarised() {
    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let mut history = conversation(thread, 2, 100);
    // A big result of an earlier turn, then the latest message.
    let mut result = row(thread, MessageRole::ToolResult, &"r".repeat(40_000), 15);
    result.tool_call_id = Some("c".to_owned());
    result.tool_is_error = Some(false);
    history.insert(2, result);
    let model = say("never");
    let (request, state) = run(&model, &db, thread, &history, &Events::new()).await;
    assert_eq!(state, Compaction::default());
    assert!(model.seen.lock().unwrap().is_empty());
    let result = request
        .messages
        .iter()
        .find_map(|m| match &m.role {
            ChatRole::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        })
        .unwrap();
    assert!(result.contains("left out"), "{result}");
}

#[tokio::test]
async fn a_conversation_too_long_for_the_context_is_summarised_and_the_summary_is_stored() {
    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let history = conversation(thread, 12, 1500);
    store(&db, &history);
    let model = say("The person asked twelve things; task 0 was about oranges.");
    let events = Events::new();
    let (request, state) = run(&model, &db, thread, &history, &events).await;

    assert!(state.used && state.wrote && !state.failed, "{state:?}");
    assert_eq!(
        events.states(),
        ["chat-compaction:summarising", "chat-compaction:done"]
    );
    // The older rows are about 36000 characters: a context of 8192 tokens takes them in two calls.
    assert!(model.seen.lock().unwrap().len() <= 2);
    // The latest message stays, word for word, and the older ones are gone from the request.
    assert_eq!(last_text(&request), "the latest question");
    assert!(request.messages.len() < 25);
    assert!(request
        .system_prompt
        .as_deref()
        .unwrap()
        .contains("task 0 was about oranges"));
    assert!(crate::chat::turn::fit::estimate_tokens(&request) <= LIMIT);

    // The row is in the thread, after the last row it covers; the covered rows are as they were.
    let rows = stored(&db, thread);
    let summary = rows
        .iter()
        .find(|row| row.role == MessageRole::Summary)
        .expect("a summary row");
    assert_eq!(rows.len(), history.len() + 1);
    let covered = rows
        .iter()
        .position(|row| Some(row.id) == summary.parent_id)
        .unwrap();
    assert_eq!(rows[covered + 1].id, summary.id);
    assert!(rows[covered].role == MessageRole::Assistant);
    for original in &history {
        assert!(rows
            .iter()
            .any(|row| row.id == original.id && row.content == original.content));
    }
}

#[tokio::test]
async fn the_latest_message_is_never_part_of_what_is_summarised() {
    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let mut history = conversation(thread, 12, 1500);
    history.last_mut().unwrap().content = "the very last question".to_owned();
    let model = say("summary");
    run(&model, &db, thread, &history, &Events::new()).await;
    let seen = model.seen.lock().unwrap();
    assert!(!seen[0].messages[0]
        .content
        .contains("the very last question"));
}

#[tokio::test]
async fn a_stored_summary_is_not_used_while_the_request_fits_without_it() {
    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let mut history = conversation(thread, 4, 100);
    let summary = ChatMessage {
        parent_id: Some(history[3].id),
        ..row(thread, MessageRole::Summary, "old summary", 3)
    };
    history.insert(4, summary);
    let model = say("never");
    let (request, state) = run(&model, &db, thread, &history, &Events::new()).await;
    assert_eq!(state, Compaction::default());
    assert_eq!(request.messages.len(), 9);
    assert!(!request
        .system_prompt
        .as_deref()
        .unwrap()
        .contains("old summary"));
}

#[tokio::test]
async fn a_stored_summary_stands_in_for_what_it_covers_when_that_is_needed_and_enough() {
    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let mut history = conversation(thread, 12, 1500);
    // A summary of the first ten turns (20 rows) stored after the tenth answer.
    let summary = ChatMessage {
        parent_id: Some(history[19].id),
        ..row(thread, MessageRole::Summary, "ten turns, stored", 191)
    };
    history.insert(20, summary);
    let model = say("never");
    let (request, state) = run(&model, &db, thread, &history, &Events::new()).await;
    assert!(state.used && !state.wrote && !state.failed, "{state:?}");
    assert!(model.seen.lock().unwrap().is_empty());
    assert_eq!(request.messages.len(), 5);
    assert!(request
        .system_prompt
        .as_deref()
        .unwrap()
        .contains("ten turns, stored"));
    assert!(request.messages[0].content.starts_with("task 10 "));
}

#[tokio::test]
async fn when_the_conversation_outgrows_the_context_again_the_earlier_summary_is_summarised_too() {
    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let mut history = conversation(thread, 30, 1500);
    let summary = ChatMessage {
        parent_id: Some(history[9].id),
        ..row(thread, MessageRole::Summary, "the first five turns", 91)
    };
    history.insert(10, summary);
    store(&db, &history);
    let model = say("the first five turns and what followed");
    let (request, state) = run(&model, &db, thread, &history, &Events::new()).await;
    assert!(state.wrote, "{state:?}");
    let seen = model.seen.lock().unwrap();
    assert!(seen[0].messages[0]
        .content
        .starts_with("Summary so far:\nthe first five turns\n\nMessages since then:"));
    assert!(request
        .system_prompt
        .as_deref()
        .unwrap()
        .contains("the first five turns and what followed"));
    assert_eq!(last_text(&request), "the latest question");
    // Two summaries are in the thread now; a request uses the one that covers more.
    let rows = stored(&db, thread);
    assert_eq!(
        rows.iter()
            .filter(|r| r.role == MessageRole::Summary)
            .count(),
        2
    );
    let latest = crate::chat::compact::select::latest_summary(&rows).unwrap();
    assert_eq!(
        rows[latest.summary].content,
        "the first five turns and what followed"
    );
}

#[tokio::test]
async fn a_summary_that_cannot_be_written_leaves_the_history_alone_and_says_so() {
    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let history = conversation(thread, 12, 1500);
    store(&db, &history);
    let events = Events::new();
    let (request, state) = run(&silent(), &db, thread, &history, &events).await;
    assert!(state.failed && !state.used && !state.wrote, "{state:?}");
    assert_eq!(events.states().last().unwrap(), "chat-compaction:failed");
    assert_eq!(stored(&db, thread).len(), history.len());
    // The request is what it was (nothing to leave out here), and the engine will say if it is too large.
    assert_eq!(request.messages.len(), 25);
}

#[tokio::test]
async fn one_turn_that_alone_is_too_large_has_nothing_to_summarise() {
    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let history = vec![row(thread, MessageRole::User, &"q".repeat(60_000), 1)];
    let model = say("never");
    let events = Events::new();
    let (_, state) = run(&model, &db, thread, &history, &events).await;
    assert_eq!(state, Compaction::default());
    assert!(model.seen.lock().unwrap().is_empty());
    assert!(events.states().is_empty());
}

#[tokio::test]
async fn ten_browser_pages_in_a_small_context_make_no_request_too_large() {
    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let mut history = vec![row(
        thread,
        MessageRole::User,
        "find the cheapest of these ten shops",
        0,
    )];
    for page in 0..10 {
        let mut call = row(thread, MessageRole::ToolCall, "", 1 + page * 3);
        call.tool_name = Some("browser_open".to_owned());
        call.tool_call_id = Some(format!("c{page}"));
        call.tool_input = Some(format!(r#"{{"url":"https://shop{page}.example"}}"#));
        call.tool_source = Some("action".to_owned());
        let mut result = row(
            thread,
            MessageRole::ToolResult,
            &"p".repeat(30_000),
            2 + page * 3,
        );
        result.tool_call_id = Some(format!("c{page}"));
        result.tool_is_error = Some(false);
        history.extend([
            call,
            result,
            row(
                thread,
                MessageRole::Assistant,
                &format!("shop {page} costs {page} euros"),
                3 + page * 3,
            ),
        ]);
    }
    history.push(row(
        thread,
        MessageRole::User,
        "so which one is cheapest?",
        100,
    ));
    let model = say("never");
    let (request, _) = run(&model, &db, thread, &history, &Events::new()).await;
    assert!(crate::chat::turn::fit::estimate_tokens(&request) <= LIMIT);
    // The model still sees what it concluded about every shop.
    let text: String = request
        .messages
        .iter()
        .map(|m| m.content.as_str())
        .collect();
    assert!(text.contains("shop 0 costs 0 euros") && text.contains("shop 9 costs 9 euros"));
}

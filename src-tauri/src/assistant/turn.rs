//! One spoken round of the voice assistant (spec 066 T041, T054, research R12).
//!
//! A round runs through the same `chat::send::start_turn` as a typed message, with
//! `origin = 'voice'`, so tools, approval levels, guardrails and limits stay those of the chat
//! (FR-020). Its `emit` closure does two things with every chat event: forward it to the window
//! (the chat shows the spoken conversation live) and feed it to a [`Bridge`], which turns token
//! deltas into speakable sentences (`assistant::speakable`, FR-021, FR-022) and the turn's progress
//! into events for `assistant::conversation`.
//!
//! The [`Bridge`] is pure and tested on its own; [`start`] adds what needs the app: a loaded model,
//! the wait for a chat turn that already runs (R12) and the call to `start_turn`. The model is
//! chosen exactly as for a typed turn (FR-020): whether the agent is driven by text or by voice
//! makes no difference, so there is no model setting of its own.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::chat::default_model::use_session_model;
use crate::chat::events::{
    EVENT_CHAT_MESSAGE_COMPLETE, EVENT_CHAT_TOKEN, EVENT_CHAT_TOOL_AVAILABILITY,
    EVENT_CHAT_TOOL_CALL, EVENT_CHAT_TOOL_RESULT, EVENT_CHAT_TURN_COMPLETE, EVENT_CHOICE_REQUEST,
    EVENT_TOOL_PERMISSION_REQUEST,
};
use crate::chat::reply_language::ReplyLanguage;
use crate::chat::send::{start_turn, SendMessageResult, StartTurn};
use crate::chat::session::ChatState;
use crate::error::{HolziError, Result};
use crate::storage::chat_threads::ThreadOrigin;

use super::conversation::{Event, OpenQuestion, QuestionKind};
use super::phrases;
use super::speakable::{Clock, Speech};

/// How long a round waits for a chat turn that already runs in a window (research R12).
pub const BUSY_WAIT: Duration = Duration::from_secs(10);
const BUSY_POLL: Duration = Duration::from_millis(200);

/// What a round hands on, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// A sentence to speak.
    Say(String),
    /// An event for `assistant::conversation`.
    Event(Event),
    /// The agent-profile notice was said; the conversation does not say it again.
    SpokeDelegateNotice,
}

/// Turns the chat events of one round into sentences and conversation events.
#[derive(Debug)]
pub struct Bridge {
    language: String,
    speech: Speech,
    answer: String,
    delegate_notice_due: bool,
    ended: bool,
    /// Questions asked and not seen answered yet.
    asked: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Token {
    delta: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Availability {
    state: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Approval {
    request_id: Uuid,
    tool_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Choice {
    request_id: Uuid,
    question: Option<String>,
    #[serde(default)]
    value: String,
}

impl Bridge {
    /// `delegate_notice_due`: the conversation has not said the delegate notice yet.
    pub fn new(language: &str, clock: Clock, delegate_notice_due: bool) -> Self {
        Self {
            language: language.to_owned(),
            speech: Speech::new(language, clock),
            answer: String::new(),
            delegate_notice_due,
            ended: false,
            asked: Vec::new(),
        }
    }

    /// The full text of the answer as the model wrote it; the transcript keeps it (T125).
    pub fn answer(&self) -> &str {
        &self.answer
    }

    /// Handles one chat event of this round.
    pub fn on_event(&mut self, name: &str, payload: &Value) -> Vec<Output> {
        if self.ended {
            return Vec::new();
        }
        let mut out = Vec::new();
        // The turn goes on, or ends, only once its questions were answered: in the window until
        // PR E answers them by voice, or by the end of the turn.
        let goes_on = matches!(
            name,
            EVENT_CHAT_TOKEN
                | EVENT_CHAT_TOOL_CALL
                | EVENT_CHAT_TOOL_RESULT
                | EVENT_CHAT_TURN_COMPLETE
        );
        if goes_on {
            out.extend(
                self.asked
                    .drain(..)
                    .map(|id| Output::Event(Event::QuestionResolved(id))),
            );
        }
        match name {
            EVENT_CHAT_TOOL_AVAILABILITY => {
                let delegate =
                    parse::<Availability>(payload).is_some_and(|a| a.state == "delegate");
                if delegate && self.delegate_notice_due {
                    self.delegate_notice_due = false;
                    let notice = phrases::delegate_notice(&self.language);
                    say_all(vec![notice], &mut out);
                    out.push(Output::SpokeDelegateNotice);
                }
            }
            EVENT_CHAT_TOKEN => {
                if let Some(token) = parse::<Token>(payload) {
                    self.answer.push_str(&token.delta);
                    say_all(self.speech.push(&token.delta), &mut out);
                }
            }
            // A message of the tool loop ended: what it said so far is a sentence of its own.
            EVENT_CHAT_MESSAGE_COMPLETE => {
                if !self.answer.is_empty() && !self.answer.ends_with('\n') {
                    self.answer.push('\n');
                    say_all(self.speech.push("\n"), &mut out);
                }
            }
            EVENT_TOOL_PERMISSION_REQUEST => {
                if let Some(approval) = parse::<Approval>(payload) {
                    self.asked.push(approval.request_id.to_string());
                    out.push(Output::Event(Event::Question(OpenQuestion {
                        id: approval.request_id.to_string(),
                        kind: QuestionKind::Approval,
                        text: approval.tool_name,
                    })));
                }
            }
            EVENT_CHOICE_REQUEST => {
                if let Some(choice) = parse::<Choice>(payload) {
                    self.asked.push(choice.request_id.to_string());
                    out.push(Output::Event(Event::Question(OpenQuestion {
                        id: choice.request_id.to_string(),
                        kind: QuestionKind::Choice,
                        text: choice.question.unwrap_or(choice.value),
                    })));
                }
            }
            // Fires once per turn, also after an error or an abort.
            EVENT_CHAT_TURN_COMPLETE => {
                say_all(self.speech.finish(), &mut out);
                out.push(Output::Event(Event::TurnEnded));
                self.ended = true;
            }
            _ => {}
        }
        out
    }
}

/// Every batch of sentences is announced to the conversation first: the first words of the answer,
/// or more after the output ran dry.
fn say_all(sentences: Vec<String>, out: &mut Vec<Output>) {
    if sentences.is_empty() {
        return;
    }
    out.push(Output::Event(Event::FirstWords));
    out.extend(sentences.into_iter().map(Output::Say));
}

fn parse<T: for<'de> Deserialize<'de>>(payload: &Value) -> Option<T> {
    serde_json::from_value(payload.clone()).ok()
}

/// What a spoken round tells the model on top of the chat's own instruction (FR-021): that its
/// answer is read aloud, so it can read texts aloud and goes on to their end without asking, in
/// the language of the conversation. Every model gets it, a delegate too.
pub fn voice_instruction(language: &str) -> Option<String> {
    let text = super::lexicon::lexicon().say("prompt.voice", language, &[]);
    (!text.trim().is_empty()).then_some(text)
}

/// What [`start`] needs for a round.
pub struct VoiceTurn {
    /// The thread of the conversation; `None` creates one.
    pub thread_id: Option<Uuid>,
    pub text: String,
    /// The language of the conversation (de, en, fr, es, it, pt).
    pub language: String,
    pub clock: Clock,
    /// The conversation has not said the delegate notice yet.
    pub delegate_notice_due: bool,
    /// Cancels a pending start without aborting another chat operation.
    pub cancel: CancellationToken,
}

/// Receives the [`Output`]s of a round, from any thread.
pub type Sink = Arc<dyn Fn(Output) + Send + Sync>;

/// How [`start`] ended.
#[derive(Debug)]
pub enum Started {
    /// The turn runs; its outputs arrive at the sink until [`Event::TurnEnded`].
    Running(SendMessageResult),
    /// A chat turn in a window did not finish within [`BUSY_WAIT`]; the round did not start and the
    /// sink got the notice and [`Event::TurnEnded`].
    GaveUp,
    /// The round was cancelled before it could start.
    Cancelled,
}

/// Starts a spoken round. On an error the sink gets nothing further; the caller ends the round.
pub async fn start(app: &AppHandle, turn: VoiceTurn, sink: Sink) -> Result<Started> {
    match wait_until_free(app, &turn.language, &sink, &turn.cancel).await {
        WaitOutcome::GaveUp => return Ok(Started::GaveUp),
        WaitOutcome::Cancelled => return Ok(Started::Cancelled),
        WaitOutcome::Available => {}
    }
    if turn.cancel.is_cancelled() {
        return Ok(Started::Cancelled);
    }
    ensure_model(app, turn.thread_id).await?;
    if turn.cancel.is_cancelled() {
        return Ok(Started::Cancelled);
    }

    let bridge = Arc::new(Mutex::new(Bridge::new(
        &turn.language,
        turn.clock,
        turn.delegate_notice_due,
    )));
    let window = app.clone();
    let to_sink = Arc::clone(&sink);
    let emit = Arc::new(move |name: &'static str, payload: Value| {
        let _ = window.emit(name, payload.clone());
        let outputs = bridge
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .on_event(name, &payload);
        for output in outputs {
            to_sink(output);
        }
    });
    let result = start_turn(
        app,
        StartTurn {
            thread_id: turn.thread_id,
            content: turn.text,
            origin: Some(ThreadOrigin::Voice),
            idempotency_key: Uuid::new_v4().to_string(),
            system_prompt: voice_instruction(&turn.language),
            utc_offset_minutes: Some(turn.clock.utc_offset_minutes),
            reply_language: ReplyLanguage::from_code(&turn.language),
            max_new_tokens: None,
            reasoning_option: None,
            attachments: Vec::new(),
            emit,
        },
    )
    .await?;
    sink(Output::Event(Event::ThreadStarted(result.thread_id)));
    Ok(Started::Running(result))
}

/// Waits while a chat turn runs in a window: says once that haex is busy, gives up after
/// [`BUSY_WAIT`] (research R12). Returns whether the chat is free.
enum WaitOutcome {
    Available,
    GaveUp,
    Cancelled,
}

async fn wait_until_free(
    app: &AppHandle,
    language: &str,
    sink: &Sink,
    cancel: &CancellationToken,
) -> WaitOutcome {
    if cancel.is_cancelled() {
        return WaitOutcome::Cancelled;
    }
    let chat = app.state::<ChatState>();
    if !chat.operation_busy() {
        return WaitOutcome::Available;
    }
    sink(Output::Event(Event::FirstWords));
    sink(Output::Say(phrases::busy(language)));
    let mut waited = Duration::ZERO;
    while waited < BUSY_WAIT {
        tokio::select! {
            _ = cancel.cancelled() => return WaitOutcome::Cancelled,
            _ = tokio::time::sleep(BUSY_POLL) => {}
        }
        waited += BUSY_POLL;
        if !chat.operation_busy() {
            return WaitOutcome::Available;
        }
    }
    sink(Output::Event(Event::FirstWords));
    sink(Output::Say(phrases::busy_gave_up(language)));
    sink(Output::Event(Event::TurnEnded));
    WaitOutcome::GaveUp
}

/// Makes sure the model of the conversation's session is loaded, by the same rule as the chat
/// (`use_session_model`): a continued session answers with the model it used last, a new one with
/// the model picked last in the model menu. Text or voice makes no difference (FR-020); the model
/// stays loaded for the chat too.
async fn ensure_model(app: &AppHandle, thread_id: Option<Uuid>) -> Result<()> {
    let resolved = use_session_model(app.clone(), app.state(), app.state(), thread_id).await?;
    if resolved.model_id.is_none() {
        return Err(HolziError::InvalidInput {
            reason: "no model loaded".into(),
        });
    }
    Ok(())
}

//! Asking the user from the tool round (spec 046, research R1/R4). After a round's calls ran, every
//! result that carries a [`ChoiceRequest`] is put to the user, one after the other, and replaced by
//! the outcome: an action runs again with the answer in its field (no second approval, FR-012; it
//! may ask again), `ask_user` gets the answer as its result, a declined question ends as
//! `declined_by_user`. Runs before the round's cancel check, so a turn cancelled while a question is
//! open still leaves no rows.

use serde_json::{json, Value};

use super::tool_round::ExecutedCall;
use super::TurnRunner;
use crate::chat::choices::ChoiceAnswer;
use crate::chat::events::{ChoiceRequestEvent, EVENT_CHOICE_REQUEST};
use crate::chat::tools::{ChoiceRequest, ToolResult};

/// The result of a question the user declined; the chat words it like `denied_by_user`.
pub(crate) const DECLINED_BY_USER: &str = "declined_by_user";

impl TurnRunner<'_> {
    /// Replaces each result that asks to choose by what the user answered. Stops at once when the
    /// turn is cancelled while waiting; the round then ends without rows.
    pub(super) async fn resolve_choices(&mut self, executed: &mut [ExecutedCall]) {
        for (call, result, source) in executed.iter_mut() {
            let Some(mut choice) = result.choice.take() else {
                continue;
            };
            let asked_for = choice.value.clone();
            let mut input = call.input.clone();
            loop {
                let Some(answer) = self.ask(&call.name, choice.clone()).await else {
                    return;
                };
                let (Some(field), Some(value)) = (choice.field.clone(), answer_value(&answer))
                else {
                    *result = answered(choice.field.is_none(), answer);
                    break;
                };
                set_field(&mut input, &field, &value);
                let tool = self
                    .chat_state
                    .tool_registry
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&call.name);
                let Some(tool) = tool else {
                    *result = ToolResult::error(format!("unknown tool: {}", call.name));
                    break;
                };
                let mut next = source
                    .run(tool.as_ref(), input.clone(), self.cancel.clone())
                    .await;
                if let Some(again) = next.choice.take() {
                    choice = again;
                    continue;
                }
                *result = with_choice_note(next, &asked_for, &value);
                break;
            }
        }
    }

    /// Emits one question and waits for its answer; `None` when the turn was cancelled meanwhile.
    async fn ask(&mut self, tool_name: &str, choice: ChoiceRequest) -> Option<ChoiceAnswer> {
        let (request_id, receiver) = self.chat_state.pending_choices.open();
        self.emit_event(
            EVENT_CHOICE_REQUEST,
            ChoiceRequestEvent {
                request_id,
                thread_id: self.thread_id,
                tool_name: tool_name.to_owned(),
                choice,
            },
        );
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => {
                self.chat_state.pending_choices.cancel(request_id);
                None
            }
            // A dropped sender means `abort_turn` gave the question up.
            answer = receiver => answer.ok(),
        }
    }
}

/// The value an answer puts into the action's field; `None` for a declined question.
fn answer_value(answer: &ChoiceAnswer) -> Option<String> {
    match answer {
        ChoiceAnswer::Option { value } => Some(value.clone()),
        ChoiceAnswer::Text { text } => Some(text.clone()),
        ChoiceAnswer::Cancel => None,
    }
}

/// The result of a question that runs nothing again: `ask_user`'s answer, or a declined question.
fn answered(own_question: bool, answer: ChoiceAnswer) -> ToolResult {
    match answer {
        ChoiceAnswer::Option { value } if own_question => {
            ToolResult::ok(json!({ "answer": value }).to_string())
        }
        ChoiceAnswer::Text { text } if own_question => {
            ToolResult::ok(json!({ "answer": text, "freeText": true }).to_string())
        }
        _ => ToolResult::error(DECLINED_BY_USER),
    }
}

fn set_field(input: &mut Value, field: &str, value: &str) {
    match input.as_object_mut() {
        Some(object) => {
            object.insert(field.to_owned(), Value::String(value.to_owned()));
        }
        None => *input = json!({ field: value }),
    }
}

/// The action's own result plus what was asked and answered, so the stored result shows the
/// choice after the chat is reopened (FR-010).
fn with_choice_note(result: ToolResult, asked_for: &str, answer: &str) -> ToolResult {
    let inner =
        serde_json::from_str::<Value>(&result.content).unwrap_or(Value::String(result.content));
    let content = json!({
        "result": inner,
        "choice": { "value": asked_for, "answer": answer },
    })
    .to_string();
    if result.is_error {
        ToolResult::error(content)
    } else {
        ToolResult::ok(content)
    }
}

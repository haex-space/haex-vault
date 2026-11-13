//! One call of a measured step, planned as `chat/turn/plan_calls.rs` plans it: a tool the registry
//! holds, one of holzi's actions only when the step offered it, the guard, the check before the
//! dialog (spec 088), then the dialog with a played user; a call that runs is answered by the
//! fixture world (moved out of `runner.rs`, spec 088 T002).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use tokio_util::sync::CancellationToken;

use crate::adapters::types::{ChatMessage, ToolCall, ToolSpec};
use crate::chat::tools::ask_user::ASK_USER_TOOL_NAME;
use crate::chat::tools::find_actions::FIND_ACTIONS_TOOL_NAME;
use crate::chat::tools::offer::{found_tools, widen_offer};
use crate::chat::tools::permission::{decide_call, PermissionMode};
use crate::chat::tools::reversible::{before_dialog, Check, Verdict};
use crate::chat::tools::route::RouteCall;
use crate::chat::tools::{RiskClass, ToolRegistry};
use crate::chat::turn::guard::{turn_away, Seen};
use crate::chat::turn::{callable, denied_answer, unknown_tool_answer};

use super::consent::{play, CallEnding};
use super::mail_names::MailNames;
use super::report::{CheckRecord, CheckVerdict};
use super::scoring::{ToolFacts, Trace, TraceCall};
use super::world::{Answer, FixtureWorld};
use super::Request;

/// What the calls of one step share.
pub(super) struct StepCalls<'a> {
    pub item: &'a Request,
    pub registry: &'a Mutex<ToolRegistry>,
    pub names: &'a MailNames,
    pub effects: &'a HashMap<String, ToolFacts>,
    pub world: &'a FixtureWorld,
    /// The offer of the step.
    pub offer: &'a [ToolSpec],
    /// What the model saw before the step.
    pub seen: &'a Seen,
    /// The tools a search never takes away.
    pub kept: &'a HashSet<String>,
    pub cancel: &'a CancellationToken,
    /// The routing call's model, when the run routes; only then a final call is checked.
    pub route: Option<&'a RouteCall<'a>>,
    /// The request's conversation before the step.
    pub history: &'a [ChatMessage],
}

/// The checks of one request: the tools already checked (spec 088 FR-006) and what each check
/// answered, in the set's names.
#[derive(Debug, Default)]
pub(super) struct RequestChecks {
    checked: HashSet<String>,
    pub done: Vec<CheckRecord>,
}

impl RequestChecks {
    fn record(&mut self, check: &Check, names: &MailNames) {
        let (verdict, use_tool) = match &check.verdict {
            Verdict::Final => (CheckVerdict::Final, None),
            Verdict::Use(candidate) => (
                CheckVerdict::Use,
                Some(names.in_set(&candidate.name).to_owned()),
            ),
            Verdict::NoVerdict => (CheckVerdict::NoVerdict, None),
        };
        self.done.push(CheckRecord {
            tool: names.in_set(&check.tool).to_owned(),
            verdict,
            use_tool,
            ms: check.ms,
        });
    }
}

/// What a call gives back to the model.
pub(super) enum Back {
    /// Its answer, holzi's or the world's.
    Answer(Answer),
    /// The question of `ask_user`, which ends the request.
    Question(String),
}

impl StepCalls<'_> {
    /// Plans `call`, answers it, records it in `trace` and `checks`, and widens `next` after a
    /// search.
    pub(super) async fn answer(
        &self,
        call: &ToolCall,
        trace: &mut Trace,
        next: &mut Vec<ToolSpec>,
        checks: &mut RequestChecks,
    ) -> Back {
        let name = self.names.in_set(&call.name);
        let lock = || self.registry.lock().unwrap_or_else(|e| e.into_inner());
        let (tool, refusal) = {
            let registry = lock();
            let tool = registry
                .get(&call.name)
                .filter(|tool| callable(tool.as_ref(), self.offer));
            let refusal = match &tool {
                None => Some((unknown_tool_answer(&call.name, self.offer), false)),
                Some(tool) => {
                    turn_away(&call.input, tool.as_ref(), self.seen, &registry, self.offer)
                        .map(|answer| (answer, tool.risk_class() == RiskClass::Risky))
                }
            };
            (tool, refusal)
        };
        let tool = match (tool, refusal) {
            (Some(tool), None) => tool,
            (_, refusal) => return self.refused(call, trace, refusal.unwrap_or_default()),
        };
        let check = before_dialog(
            self.route,
            PermissionMode::Auto,
            tool.as_ref(),
            self.registry,
            self.offer,
            self.history,
            &mut checks.checked,
        )
        .await;
        if let Some(check) = check {
            checks.record(&check, self.names);
            if let Some(content) = check.turned_away() {
                return self.refused(call, trace, (content, true));
            }
        }
        let played = play(
            self.item,
            decide_call(PermissionMode::Auto, tool.as_ref()),
            &trace.calls,
            name,
            &call.input,
            self.effects,
        );
        let risky = tool.risk_class() == RiskClass::Risky;
        if call.name == ASK_USER_TOOL_NAME {
            // The question back; the rest of the batch still runs, as in the chat.
            trace.calls.push(TraceCall {
                tool: name.to_owned(),
                args: call.input.clone(),
                result: None,
                ending: played.ending,
                unasked: played.unasked,
                risky,
            });
            let question = call.input.get("question").and_then(|q| q.as_str());
            return Back::Question(question.unwrap_or_default().to_owned());
        }
        let answer = if played.ending == CallEnding::Stopped {
            Answer {
                content: denied_answer(),
                is_error: true,
            }
        } else if call.name == FIND_ACTIONS_TOOL_NAME {
            let result = tool.execute(call.input.clone(), self.cancel.clone()).await;
            if let Some(found) = found_tools(&result.content).filter(|_| !result.is_error) {
                widen_offer(&lock(), next, &found, self.kept);
            }
            Answer {
                content: result.content,
                is_error: result.is_error,
            }
        } else {
            self.world.answer(name, &call.input)
        };
        trace.calls.push(TraceCall {
            tool: name.to_owned(),
            args: call.input.clone(),
            result: Some(answer.clone()),
            ending: played.ending,
            unasked: played.unasked,
            risky,
        });
        Back::Answer(answer)
    }

    /// Records `call` as turned away before it ran, with holzi's answer `content`; `risky` when
    /// its tool cannot be undone.
    fn refused(
        &self,
        call: &ToolCall,
        trace: &mut Trace,
        (content, risky): (String, bool),
    ) -> Back {
        let answer = Answer {
            content,
            is_error: true,
        };
        trace.calls.push(TraceCall {
            tool: self.names.in_set(&call.name).to_owned(),
            args: call.input.clone(),
            result: Some(answer.clone()),
            ending: CallEnding::TurnedAway,
            unasked: None,
            risky,
        });
        Back::Answer(answer)
    }
}

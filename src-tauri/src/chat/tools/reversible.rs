//! The reversible way first (spec 088): before the dialog asks about a call that cannot be undone,
//! holzi asks the model one short question, like the routing call: did the user ask for exactly
//! this, and which offered tool of the same area does what the user asked in a way that can be
//! undone. If the user did not ask for it and a tool is named, the call is turned away with an
//! answer naming that tool; in every other case the dialog asks as before.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::adapters::types::{ChatMessage, ChatRequest, ChatRole, ToolSpec};

use super::ask_user::ASK_USER_TOOL_NAME;
use super::domains::domain_of;
use super::offer::earlier_user_messages;
use super::permission::PermissionMode;
use super::route::{ask, RouteCall};
use super::{RiskClass, Tool, ToolRegistry};
use crate::chat::turn::guard::{next_step, Failure};

// ponytail: wording and token limit per contracts/check-question.md; tune them only on the chain set
// (spec 087 rule) and record each change in specs/088-reversible-delete/results.md.
/// At most this many new tokens: one tool name.
pub const MAX_CHECK_TOKENS: usize = 16;

/// An offered tool that changes data without being final, in the area of the proposed call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The model-facing name, as offered.
    pub name: String,
    /// The tool's English title.
    pub title: String,
}

/// What the check answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The user asked for exactly this final action: the dialog asks.
    Final,
    /// The user did not; this candidate does it reversibly: the call is turned away.
    Use(Candidate),
    /// `none`, an answer holzi cannot read, an error or no answer in time: the dialog asks.
    NoVerdict,
}

/// One check of a proposed final call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// The proposed tool's model-facing name.
    pub tool: String,
    pub verdict: Verdict,
    pub ms: u64,
}

impl Check {
    /// The answer to the model for a call the check turns away; `None` when the dialog asks.
    pub fn turned_away(&self) -> Option<String> {
        let Verdict::Use(candidate) = &self.verdict else {
            return None;
        };
        Some(format!(
            "turned away: `{}` cannot be undone and the user did not ask for that; nothing ran and \
             nothing changed. {}",
            self.tool,
            next_step(Failure::NotFinal {
                candidate: &candidate.name,
                title: &candidate.title,
            })
        ))
    }
}

/// The English title of `tool`: an extension tool's, an action's, or its name.
pub fn title_of(tool: &dyn Tool) -> String {
    let title = match (tool.origin(), tool.action_definition()) {
        (Some(origin), _) => origin.tool_title_en,
        (None, Some(def)) => def.titles.en.clone(),
        (None, None) => String::new(),
    };
    if title.is_empty() {
        tool.name().to_owned()
    } else {
        title
    }
}

/// Whether a proposed call of `tool` in `mode` is checked at all: it cannot be undone, it does not
/// ask the user itself (spec 060), and Plan mode does not refuse it anyway (FR-008).
pub fn applies(tool: &dyn Tool, mode: PermissionMode) -> bool {
    tool.risk_class() == RiskClass::Risky && !tool.confirms_itself() && mode != PermissionMode::Plan
}

/// The tools `offered` in the step that share the domain of `tool` and change data without being
/// final (research R3); empty when `tool` belongs to no domain.
pub fn candidates(
    tool: &dyn Tool,
    registry: &ToolRegistry,
    offered: &[ToolSpec],
) -> Vec<Candidate> {
    let Some(domain) = domain_of(tool) else {
        return Vec::new();
    };
    registry
        .iter()
        .filter(|other| {
            other.risk_class() == RiskClass::Change
                && offered.iter().any(|spec| spec.name == other.name())
                && domain_of(other.as_ref()).as_deref() == Some(domain.as_str())
        })
        .map(|other| Candidate {
            name: other.name().to_owned(),
            title: title_of(other.as_ref()),
        })
        .collect()
}

/// The system message of the check (contracts/check-question.md); it names no area and no concept
/// such as a trash (spec 087 FR-019).
pub fn check_prompt(tool: &str, title: &str, candidates: &[Candidate]) -> String {
    let mut prompt = format!(
        "The assistant wants to call `{tool}` ({title}). It cannot be undone, so it is only right \
         when the user explicitly says permanently, for good or never to be restored. Saying only \
         what to do is not enough.\nIf the user's message below says so explicitly, answer: \
         final\nOtherwise answer with the tool from this list that does what the user asked and \
         can be undone:\n"
    );
    for candidate in candidates {
        prompt.push_str(&format!("{}: {}\n", candidate.name, candidate.title));
    }
    prompt.push_str(&format!(
        "Never answer `{tool}`. Answer with one word: final, or one tool name from the list."
    ));
    prompt
}

fn text(role: ChatRole, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_owned(),
        attachments: Vec::new(),
    }
}

/// The question the assistant asked last before the latest user message: its last text answer when
/// it ends with a question mark, or the question of an `ask_user` call that came after it.
fn last_question(before: &[ChatMessage]) -> Option<String> {
    for message in before.iter().rev() {
        match &message.role {
            ChatRole::Assistant if !message.content.trim().is_empty() => {
                let said = message.content.trim();
                return said.ends_with('?').then(|| said.to_owned());
            }
            ChatRole::ToolCall { name, input, .. } if name == ASK_USER_TOOL_NAME => {
                return input["question"].as_str().map(str::to_owned);
            }
            ChatRole::User => return None,
            _ => {}
        }
    }
    None
}

/// What the check reads of a conversation (research R4): the user's messages, the earlier ones
/// bounded as for the routing call, and before the latest one the assistant's last question, so a
/// short confirmation is read in context. Tool calls and results are left out. Also gives the
/// characters of the earlier messages, for the timeout.
pub fn check_messages(messages: &[ChatMessage]) -> (Vec<ChatMessage>, usize) {
    let Some(latest) = messages
        .iter()
        .rposition(|message| matches!(message.role, ChatRole::User))
    else {
        return (Vec::new(), 0);
    };
    let users: Vec<&str> = messages[..=latest]
        .iter()
        .filter(|message| matches!(message.role, ChatRole::User))
        .map(|message| message.content.as_str())
        .collect();
    let earlier = earlier_user_messages(&users);
    let question = last_question(&messages[..latest]);
    let chars = earlier
        .iter()
        .chain(question.iter())
        .map(|said| said.chars().count())
        .sum();
    let mut out: Vec<ChatMessage> = earlier
        .iter()
        .map(|said| text(ChatRole::User, said))
        .collect();
    if let Some(question) = question {
        out.push(text(ChatRole::Assistant, &question));
    }
    out.push(text(ChatRole::User, &messages[latest].content));
    (out, chars)
}

/// Whether one of `candidates` already ran without error since the latest user message: the final
/// call then continues what the model did reversibly in this turn (a draft before sending), so the
/// dialog asks without a check. A call's result is the first one after it with its id: a local
/// model numbers the calls of every step from the start, so one id can recur in a turn.
pub fn candidate_ran(messages: &[ChatMessage], candidates: &[Candidate]) -> bool {
    let turn = messages
        .iter()
        .rposition(|message| matches!(message.role, ChatRole::User))
        .map_or(messages, |latest| &messages[latest + 1..]);
    turn.iter().enumerate().any(|(at, message)| {
        let ChatRole::ToolCall { id, name, .. } = &message.role else {
            return false;
        };
        candidates.iter().any(|candidate| &candidate.name == name)
            && turn[at + 1..]
                .iter()
                .find_map(|result| match &result.role {
                    ChatRole::ToolResult {
                        call_id, is_error, ..
                    } if call_id == id => Some(!is_error),
                    _ => None,
                })
                .unwrap_or(false)
    })
}

/// The model-facing name without the `x_<slug>_` prefix of an extension tool.
fn short_name(name: &str) -> &str {
    name.strip_prefix("x_")
        .and_then(|rest| rest.split_once('_'))
        .map_or(name, |(_, short)| short)
}

/// Reads the check's answer (research R5): the first word that is `final`, `none` or a candidate's
/// name, with or without its `x_<slug>_` prefix, in any case, decides; anything else is no verdict.
pub fn parse_check(reply: &str, candidates: &[Candidate]) -> Verdict {
    for word in reply
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
    {
        let word = word.to_lowercase();
        match word.as_str() {
            "final" => return Verdict::Final,
            "none" => return Verdict::NoVerdict,
            _ => {}
        }
        let named = candidates.iter().find(|candidate| {
            let name = candidate.name.to_lowercase();
            word == name || word == short_name(&name)
        });
        if let Some(candidate) = named {
            return Verdict::Use(candidate.clone());
        }
    }
    Verdict::NoVerdict
}

/// Asks the model the check for a proposed call of `tool` (title `title`): one request with no
/// tools, thinking off, at most [`MAX_CHECK_TOKENS`] new tokens, within the routing call's timeout
/// plus a second per 1000 characters of earlier messages.
pub async fn check(
    call: &RouteCall<'_>,
    messages: &[ChatMessage],
    tool: &str,
    title: &str,
    candidates: &[Candidate],
) -> Check {
    let (messages, earlier_chars) = check_messages(messages);
    let request = ChatRequest {
        model_id: call.model_id.to_owned(),
        thread_id: None,
        system_prompt: Some(check_prompt(tool, title, candidates)),
        messages,
        reasoning_requested: false,
        max_new_tokens: Some(MAX_CHECK_TOKENS),
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        autonomy_deny: Default::default(),
        reasoning_option: None,
        capabilities: None,
        sampling: call.sampling,
    };
    let start = Instant::now();
    let deadline = call.timeout + Duration::from_millis(earlier_chars as u64);
    let answered = tokio::time::timeout(deadline, ask(call, request)).await;
    let ms = start.elapsed().as_millis() as u64;
    let verdict = match &answered {
        Ok(Some(reply)) => parse_check(reply, candidates),
        Ok(None) | Err(_) => Verdict::NoVerdict,
    };
    log_check(tool, &answered, &verdict, ms);
    Check {
        tool: tool.to_owned(),
        verdict,
        ms,
    }
}

/// The check before the dialog, as the chat and the measurement run it: for a call of `tool` in
/// `mode` that [`applies`], in a turn that routes (`call`), not yet checked in this turn (FR-006),
/// with [`candidates`] none of which already ran in this turn ([`candidate_ran`]), the [`check`];
/// `None` where no check runs. The registry is not locked during the question.
pub async fn before_dialog(
    call: Option<&RouteCall<'_>>,
    mode: PermissionMode,
    tool: &dyn Tool,
    registry: &Mutex<ToolRegistry>,
    offered: &[ToolSpec],
    messages: &[ChatMessage],
    checked: &mut HashSet<String>,
) -> Option<Check> {
    let call = call?;
    if !applies(tool, mode) || checked.contains(tool.name()) {
        return None;
    }
    let candidates = {
        let registry = registry.lock().unwrap_or_else(|e| e.into_inner());
        candidates(tool, &registry, offered)
    };
    if candidates.is_empty() {
        return None;
    }
    if candidate_ran(messages, &candidates) {
        log::info!(
            "chat: no check of {}: a reversible tool of its area already ran this turn",
            tool.name()
        );
        return None;
    }
    checked.insert(tool.name().to_owned());
    Some(check(call, messages, tool.name(), &title_of(tool), &candidates).await)
}

/// One line per check: the tool, the verdict and the time; never the user's words.
fn log_check(
    tool: &str,
    answered: &Result<Option<String>, tokio::time::error::Elapsed>,
    verdict: &Verdict,
    ms: u64,
) {
    match (answered, verdict) {
        (Err(_), _) => log::warn!("chat: check of {tool} timed out after {ms} ms, asking the user"),
        (Ok(None), _) => log::warn!("chat: check of {tool} failed after {ms} ms, asking the user"),
        (Ok(Some(_)), Verdict::Use(candidate)) => log::info!(
            "chat: check of {tool} in {ms} ms: not asked for, use {}",
            candidate.name
        ),
        (Ok(Some(_)), Verdict::Final) => {
            log::info!("chat: check of {tool} in {ms} ms: asked for, asking the user")
        }
        (Ok(Some(_)), Verdict::NoVerdict) => {
            log::info!("chat: check of {tool} in {ms} ms: no verdict, asking the user")
        }
    }
}

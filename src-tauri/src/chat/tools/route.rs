//! The routing call (spec 082 FR-006, research R3, contracts/domains.md): before the first step the
//! same model names the domains a request needs, and the first step offers their tools. Any failure
//! keeps today's offer.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::adapters::types::{ChatMessage, ChatRequest, ChatRole, Sampling, StreamChunk, ToolSpec};
use crate::adapters::ProviderAdapter;
use crate::chat::session::ActiveSession;
use crate::storage::chat_messages::{ChatMessage as StoredMessage, MessageRole};
use crate::storage::providers::ProviderKind;

use super::availability::ToolAvailability;
use super::domains::{domain_of, domains, Domain, NO_DOMAIN};
use super::offer::{first_offer_in, routed_offer_in};
use super::ToolRegistry;

// ponytail: the prompt's form, the token limit and the timeouts are the ones research R3 measured
// (`analysis/router_eval.py`); change them only with a new measurement on the dev set.
/// At most this many new tokens: one or two domain names.
pub const MAX_ROUTE_TOKENS: usize = 12;
/// At most this many domains per request.
pub const MAX_ROUTED: usize = 2;
/// A local engine answers in 30 to 170 ms (research R3).
pub const LOCAL_ROUTE_TIMEOUT: Duration = Duration::from_secs(3);
/// A hosted provider's request alone can take seconds.
pub const HOSTED_ROUTE_TIMEOUT: Duration = Duration::from_secs(15);

// ponytail: only local engines route (spec 082 T053, operator 2026-11-05). Hosted models passed 101 → 102 (Opus
// 5.5), 98 → 98 (Sonnet 5.5) and 97 → 99 (Haiku 5.5) of 106 with routing, within their run-to-run noise, for one
// more request per turn; local models gained most (Qwen3.5 4B 42 → 79). Results: spec 082 results.md "Hosted".
/// Whether a chat turn with a model of `kind` makes the routing call.
pub fn routes(kind: ProviderKind) -> bool {
    kind == ProviderKind::Local
}

/// How long the routing call may take for a model of `kind`.
pub fn route_timeout(kind: ProviderKind) -> Duration {
    match kind {
        ProviderKind::Local => LOCAL_ROUTE_TIMEOUT,
        ProviderKind::ApiKey | ProviderKind::CliDelegate => HOSTED_ROUTE_TIMEOUT,
    }
}

/// The system message of the routing call: every installed domain and `none`.
pub fn routing_prompt(domains: &[Domain]) -> String {
    let mut prompt =
        String::from("Which area of the app holzi does the user's request need? Areas:\n");
    for domain in domains {
        prompt.push_str(&format!("{}: {}\n", domain.name, domain.description));
    }
    prompt.push_str(&format!(
        "{NO_DOMAIN}: anything else: general knowledge, arithmetic, the time, small talk\n"
    ));
    // Spec 087 research R11: a request that names a person or a thing, not the area, goes to every
    // area that may hold it; tuned on the routing development sets for the local models, the phone
    // model included (`runs/route/rule-answer.txt`, `runs/route/phone/`).
    prompt.push_str(
        "A request that names only a person or a thing, not the area, needs every area that can \
         hold it. Answer with the one area the request needs, or two if it names only a person or \
         a thing that two areas could hold, comma-separated, nothing else.",
    );
    prompt
}

/// The keys of the domains `reply` names, in order, at most [`MAX_ROUTED`]; a domain counts by its
/// name or its key, in any case. `none` and unknown words name nothing.
pub fn parse_route(reply: &str, domains: &[Domain]) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for word in reply
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
    {
        let word = word.to_lowercase();
        let found = domains
            .iter()
            .find(|domain| domain.name.to_lowercase() == word || domain.key.to_lowercase() == word);
        if let Some(domain) = found {
            if !keys.contains(&domain.key) {
                keys.push(domain.key.clone());
            }
        }
        if keys.len() == MAX_ROUTED {
            break;
        }
    }
    keys
}

/// What the routing call gave: the routed keys (empty: today's offer) and how long it took, if it
/// ran.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Routed {
    pub keys: Vec<String>,
    pub ms: Option<u64>,
    /// Domains the first step offers besides `keys` because the last turn used their tools
    /// ([`first_step_offer_in`]).
    pub kept: Vec<String>,
}

/// The model the routing call asks, and how long it may take.
pub struct RouteCall<'a> {
    pub adapter: &'a dyn ProviderAdapter,
    pub model_id: &'a str,
    pub timeout: Duration,
    pub sampling: Sampling,
}

impl<'a> RouteCall<'a> {
    /// The chat's model, with the timeout of its kind and its default sampling; `None` for a model
    /// that does not route ([`routes`]).
    pub fn of(session: &'a ActiveSession, model_id: &'a str) -> Option<Self> {
        routes(session.provider_kind).then(|| Self {
            adapter: session.adapter.as_ref(),
            model_id,
            timeout: route_timeout(session.provider_kind),
            sampling: Sampling::Default,
        })
    }
}

/// The text of the model's answer to `request`; `None` on an error.
pub(super) async fn ask(call: &RouteCall<'_>, request: ChatRequest) -> Option<String> {
    let mut stream = call.adapter.stream_chat(request).await.ok()?;
    let mut text = String::new();
    while let Some(item) = stream.next().await {
        match item.ok()? {
            StreamChunk::Delta { content, .. } => text.push_str(&content),
            StreamChunk::Done { .. } => break,
            _ => {}
        }
    }
    Some(text)
}

/// Asks the model which of `domains` `message` needs: one request with no tools, thinking off, at
/// most [`MAX_ROUTE_TOKENS`] new tokens. An error, a timeout or an answer without a known domain
/// gives no keys.
pub async fn route(call: &RouteCall<'_>, domains: &[Domain], message: &str) -> Routed {
    route_in(call, domains, &[], message).await
}

/// [`route`] for a message in a conversation: the user's `earlier` messages go before it, so a
/// follow-up that names nothing ("try again") is routed by what the conversation is about.
pub async fn route_in(
    call: &RouteCall<'_>,
    domains: &[Domain],
    earlier: &[String],
    message: &str,
) -> Routed {
    let request = ChatRequest {
        model_id: call.model_id.to_owned(),
        thread_id: None,
        system_prompt: Some(routing_prompt(domains)),
        messages: earlier
            .iter()
            .map(String::as_str)
            .chain([message])
            .map(|content| ChatMessage {
                role: ChatRole::User,
                content: content.to_owned(),
                attachments: Vec::new(),
            })
            .collect(),
        reasoning_requested: false,
        max_new_tokens: Some(MAX_ROUTE_TOKENS),
        tools: Vec::new(),
        autonomy_mode: Default::default(),
        autonomy_deny: Default::default(),
        reasoning_option: None,
        capabilities: None,
        sampling: call.sampling,
    };
    let start = Instant::now();
    // The model reads the kept history too: a second more per 1000 characters of it.
    let history_chars: usize = earlier.iter().map(|text| text.chars().count()).sum();
    let deadline = call.timeout + Duration::from_millis(history_chars as u64);
    let answered = tokio::time::timeout(deadline, ask(call, request)).await;
    let ms = start.elapsed().as_millis() as u64;
    let keys = match &answered {
        Ok(Some(reply)) => parse_route(reply, domains),
        Ok(None) | Err(_) => Vec::new(),
    };
    log_route(&answered, &keys, ms, deadline);
    Routed {
        keys,
        ms: Some(ms),
        kept: Vec::new(),
    }
}

/// Characters of the model's answer the log keeps; the answer is at most [`MAX_ROUTE_TOKENS`] tokens.
const LOGGED_REPLY_CHARS: usize = 40;

/// One line per routing call, so a first step that offered no tools for a request can be told
/// apart: the call ran out of time, failed, or the model named no domain. The user's message is
/// not logged, only the model's short answer.
fn log_route(
    answered: &Result<Option<String>, tokio::time::error::Elapsed>,
    keys: &[String],
    ms: u64,
    deadline: Duration,
) {
    let limit = deadline.as_millis();
    match answered {
        Err(_) => log::warn!(
            "chat: routing timed out after {ms} ms (limit {limit} ms), using the fallback offer"
        ),
        Ok(None) => log::warn!(
            "chat: routing failed after {ms} ms (limit {limit} ms), using the fallback offer"
        ),
        Ok(Some(reply)) => {
            let reply: String = reply.trim().chars().take(LOGGED_REPLY_CHARS).collect();
            log::info!(
                "chat: routing answered {reply:?} in {ms} ms (limit {limit} ms): domains {keys:?}"
            );
        }
    }
}

/// The first step's offer of a chat turn: routed when there is a routing `call`, the model can use
/// tools and domains are installed; today's [`first_offer`] otherwise. The registry is not locked
/// during the call.
pub async fn first_step_offer(
    availability: ToolAvailability,
    registry: &Mutex<ToolRegistry>,
    call: Option<&RouteCall<'_>>,
    message: &str,
) -> (Vec<ToolSpec>, Routed) {
    first_step_offer_in(availability, registry, call, &[], &[], message).await
}

/// [`first_step_offer`] for a message in a conversation (`earlier`: the user's earlier messages,
/// oldest first; `used_tools`: the tools the conversation's last turn called, [`last_turn_tools`]).
pub async fn first_step_offer_in(
    availability: ToolAvailability,
    registry: &Mutex<ToolRegistry>,
    call: Option<&RouteCall<'_>>,
    earlier: &[String],
    used_tools: &[String],
    message: &str,
) -> (Vec<ToolSpec>, Routed) {
    let lock = || registry.lock().unwrap_or_else(|e| e.into_inner());
    let installed = domains(&lock());
    let Some(call) = call.filter(|_| availability.offers_tools() && !installed.is_empty()) else {
        return (
            first_offer_in(availability, &lock(), earlier, message),
            Routed::default(),
        );
    };
    let mut routed = route_in(call, &installed, earlier, message).await;
    routed.kept = used_domains(&lock(), used_tools)
        .into_iter()
        .filter(|key| !routed.keys.contains(key))
        .collect();
    let keys: Vec<String> = routed.keys.iter().chain(&routed.kept).cloned().collect();
    let offer = routed_offer_in(availability, &lock(), &installed, &keys, earlier, message);
    (offer, routed)
}

/// The domains of the tools in `used_tools`, in order, at most [`MAX_ROUTED`]; tools of no domain
/// (the search, `ask_user`, the command tool) and tools no longer registered name none.
pub fn used_domains(registry: &ToolRegistry, used_tools: &[String]) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for key in used_tools
        .iter()
        .filter_map(|name| registry.get(name))
        .filter_map(|tool| domain_of(tool.as_ref()))
    {
        if keys.len() < MAX_ROUTED && !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// The tools the conversation's last turn called: the tool calls after the user's message before
/// `current`, the message being answered now. A follow-up such as "what does George write?" after
/// a mail answer keeps the mail tools, whatever the routing call answers for it alone.
pub fn last_turn_tools(history: &[StoredMessage], current: Uuid) -> Vec<String> {
    let rows: Vec<&StoredMessage> = history.iter().filter(|row| row.id != current).collect();
    let Some(last_user) = rows
        .iter()
        .rposition(|row| matches!(row.role, MessageRole::User))
    else {
        return Vec::new();
    };
    rows[last_user + 1..]
        .iter()
        .filter(|row| matches!(row.role, MessageRole::ToolCall))
        .filter_map(|row| row.tool_name.clone())
        .collect()
}

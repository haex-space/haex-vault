use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;

use super::availability::ToolAvailability;
use super::domains::{domains, Domain};
use super::domains_tests::{mail, registry_with};
use super::offer::first_offer;
use super::route::{
    first_step_offer, parse_route, route, route_in, route_timeout, routes, routing_prompt,
    RouteCall, Routed, MAX_ROUTE_TOKENS,
};
use crate::adapters::types::{ChatRequest, Sampling, StreamChunk};
use crate::adapters::{AdapterError, AdapterStream, ProviderAdapter, ProviderModel};
use crate::chat::eval::testing::Scripted;
use crate::storage::providers::ProviderKind;

fn installed() -> Vec<Domain> {
    domains(&registry_with(mail()))
}

fn answering(reply: &'static str) -> Scripted {
    Scripted::new(move |_| {
        vec![StreamChunk::Delta {
            content: reply.to_owned(),
            reasoning: None,
        }]
    })
}

fn call(adapter: &dyn ProviderAdapter, timeout: Duration) -> RouteCall<'_> {
    RouteCall {
        adapter,
        model_id: "m",
        timeout,
        sampling: Sampling::Deterministic,
    }
}

#[test]
fn the_prompt_lists_every_installed_domain_and_none() {
    let prompt = routing_prompt(&installed());
    assert!(
        prompt.starts_with("Which area of the app holzi does the user's request need? Areas:\n")
    );
    assert!(prompt.contains("\napps: open, close or switch apps, windows and tabs\n"));
    assert!(prompt.contains("\nmail: haex-mail: search mail, delete mail for good\n"));
    assert!(prompt
        .contains("\nnone: anything else: general knowledge, arithmetic, the time, small talk\n"));
    assert!(prompt.ends_with(
        "Answer with one area name, or two if the request could mean either, comma-separated, \
         nothing else."
    ));
}

#[test]
fn a_reply_names_up_to_two_domains_by_name_or_key_in_any_case() {
    let domains = installed();
    let parse = |reply| parse_route(reply, &domains);
    assert_eq!(parse("contacts"), ["contacts"]);
    assert_eq!(parse("mail, contacts"), ["x_mail", "contacts"]);
    assert_eq!(parse("mail, contacts, files"), ["x_mail", "contacts"]);
    assert_eq!(parse("The area is contacts."), ["contacts"]);
    assert_eq!(parse("CONTACTS"), ["contacts"]);
    assert_eq!(parse("x_mail"), ["x_mail"]);
    assert_eq!(parse("apps"), ["wm"]);
    assert_eq!(parse("contacts, contacts"), ["contacts"]);
}

#[test]
fn none_and_unknown_words_name_no_domain() {
    let domains = installed();
    assert!(parse_route("none", &domains).is_empty());
    assert!(parse_route("calendar", &domains).is_empty());
    assert!(parse_route("", &domains).is_empty());
}

#[tokio::test]
async fn the_call_asks_with_no_tools_and_few_tokens_and_times_itself() {
    crate::test_log::install();
    let model = answering("mail");
    let routed = route(
        &call(&model, Duration::from_secs(3)),
        &installed(),
        "Hat Anna geschrieben?",
    )
    .await;
    assert_eq!(routed.keys, ["x_mail"]);
    assert!(routed.ms.is_some());
    let logs = crate::test_log::all();
    assert!(logs.lines().any(|line| {
        line.contains("chat: routing answered \"mail\" in ")
            && line.contains("(limit 3000 ms): domains [\"x_mail\"]")
    }));
    let seen = model.seen.lock().unwrap();
    let request = &seen[0];
    assert!(request.tools.is_empty());
    assert_eq!(request.max_new_tokens, Some(MAX_ROUTE_TOKENS));
    assert!(!request.reasoning_requested);
    assert_eq!(request.sampling, Sampling::Deterministic);
    assert_eq!(request.messages[0].content, "Hat Anna geschrieben?");
    assert_eq!(
        request.system_prompt.as_deref(),
        Some(routing_prompt(&installed()).as_str())
    );
}

struct Broken;

#[async_trait]
impl ProviderAdapter for Broken {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }
    async fn stream_chat(&self, _: ChatRequest) -> Result<AdapterStream, AdapterError> {
        Err(AdapterError::Http {
            reason: "down".to_owned(),
        })
    }
}

struct Silent;

#[async_trait]
impl ProviderAdapter for Silent {
    async fn list_models(&self) -> Result<Vec<ProviderModel>, AdapterError> {
        Ok(Vec::new())
    }
    async fn stream_chat(&self, _: ChatRequest) -> Result<AdapterStream, AdapterError> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn an_error_a_timeout_or_no_known_domain_routes_nowhere() {
    crate::test_log::install();
    let domains = installed();
    let short = Duration::from_millis(73);
    let unknown = answering("I do not know");
    for (adapter, outcome) in [
        (&Broken as &dyn ProviderAdapter, "failed"),
        (&Silent, "timed out"),
        (&unknown, "answered \"I do not know\""),
    ] {
        let routed = route(&call(adapter, short), &domains, "x").await;
        assert!(routed.keys.is_empty());
        assert!(crate::test_log::all().lines().any(|line| {
            line.contains(&format!("chat: routing {outcome}")) && line.contains("(limit 73 ms)")
        }));
    }
}

#[tokio::test]
async fn a_failed_routing_call_keeps_today_s_offer() {
    let registry = Mutex::new(registry_with(mail()));
    let text = "Hat Anna geschrieben?";
    let (offer, routed) = first_step_offer(
        ToolAvailability::Offered,
        &registry,
        Some(&call(&Broken, Duration::from_millis(50))),
        text,
    )
    .await;
    let today = first_offer(ToolAvailability::Offered, &registry.lock().unwrap(), text);
    assert_eq!(names(&offer), names(&today));
    assert!(routed.keys.is_empty());
}

#[tokio::test]
async fn a_model_without_tools_is_not_routed() {
    let registry = Mutex::new(registry_with(mail()));
    let model = answering("mail");
    let (offer, routed) = first_step_offer(
        ToolAvailability::Unsupported,
        &registry,
        Some(&call(&model, Duration::from_secs(3))),
        "x",
    )
    .await;
    assert!(offer.is_empty());
    assert_eq!(routed, Routed::default());
    assert!(model.seen.lock().unwrap().is_empty());
}

#[test]
fn a_local_engine_gets_three_seconds_and_a_hosted_provider_fifteen() {
    assert_eq!(route_timeout(ProviderKind::Local), Duration::from_secs(3));
    assert_eq!(route_timeout(ProviderKind::ApiKey), Duration::from_secs(15));
}

fn names(offer: &[crate::adapters::types::ToolSpec]) -> Vec<String> {
    offer.iter().map(|tool| tool.name.clone()).collect()
}

#[test]
fn only_a_local_engine_routes() {
    assert!(routes(ProviderKind::Local));
    assert!(!routes(ProviderKind::ApiKey));
    assert!(!routes(ProviderKind::CliDelegate));
}

#[tokio::test]
async fn without_a_routing_call_the_offer_is_today_s() {
    let registry = Mutex::new(registry_with(mail()));
    let text = "Hat Anna geschrieben?";
    let (offer, routed) = first_step_offer(ToolAvailability::Offered, &registry, None, text).await;
    let today = first_offer(ToolAvailability::Offered, &registry.lock().unwrap(), text);
    assert_eq!(names(&offer), names(&today));
    assert_eq!(routed, Routed::default());
}

#[tokio::test]
async fn the_call_sees_the_earlier_user_messages_before_the_current_one() {
    let model = answering("mail");
    let routed = route_in(
        &call(&model, Duration::from_secs(3)),
        &installed(),
        &["Wie viele ungelesene Mails habe ich?".to_owned()],
        "Kannst du es noch mal probieren?",
    )
    .await;
    assert_eq!(routed.keys, ["x_mail"]);
    let seen = model.seen.lock().unwrap();
    let contents: Vec<&str> = seen[0]
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(
        contents,
        [
            "Wie viele ungelesene Mails habe ich?",
            "Kannst du es noch mal probieren?"
        ]
    );
}

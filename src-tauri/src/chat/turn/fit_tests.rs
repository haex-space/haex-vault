use serde_json::json;

use super::fit::{estimate_tokens, estimated, fit, size_in_chars};
use crate::adapters::types::{ChatMessage, ChatRequest, ChatRole, ToolSpec};

fn request(messages: Vec<ChatMessage>) -> ChatRequest {
    ChatRequest {
        sampling: Default::default(),
        model_id: "m".to_owned(),
        thread_id: None,
        system_prompt: Some("You operate holzi.".to_owned()),
        messages,
        reasoning_requested: false,
        max_new_tokens: None,
        tools: vec![ToolSpec {
            name: "browser_open".to_owned(),
            description: "Open a page.".to_owned(),
            input_schema: json!({ "type": "object" }),
        }],
        autonomy_mode: Default::default(),
        autonomy_deny: Vec::new(),
        reasoning_option: None,
        capabilities: None,
    }
}

fn message(role: ChatRole, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_owned(),
        attachments: Vec::new(),
    }
}

fn user(text: &str) -> ChatMessage {
    message(ChatRole::User, text)
}

fn call(id: &str) -> ChatMessage {
    message(
        ChatRole::ToolCall {
            id: id.to_owned(),
            name: "browser_open".to_owned(),
            input: json!({ "url": "https://a.de" }),
        },
        "",
    )
}

fn result(id: &str, content: &str) -> ChatMessage {
    message(
        ChatRole::ToolResult {
            call_id: id.to_owned(),
            content: content.to_owned(),
            is_error: false,
        },
        "",
    )
}

fn content_of(request: &ChatRequest, at: usize) -> &str {
    match &request.messages[at].role {
        ChatRole::ToolResult { content, .. } => content,
        other => panic!("not a result: {other:?}"),
    }
}

#[test]
fn a_request_that_fits_is_left_alone() {
    let mut request = request(vec![
        user("Hallo"),
        call("1"),
        result("1", &"x".repeat(500)),
    ]);
    let before = request.messages.clone();

    assert!(!fit(&mut request, 8192, &estimated));

    assert_eq!(request.messages, before);
}

#[test]
fn the_size_counts_prompt_messages_calls_and_tools() {
    let small = request(vec![user("Hallo")]);
    let bigger = request(vec![
        user("Hallo"),
        call("1"),
        result("1", &"x".repeat(900)),
    ]);
    assert!(size_in_chars(&bigger) >= size_in_chars(&small) + 900);
    assert!(estimate_tokens(&bigger) >= 300);
}

#[test]
fn earlier_results_are_left_out_oldest_first_until_it_fits() {
    let big = "x".repeat(8_000);
    let mut request = request(vec![
        user("Öffne a"),
        call("1"),
        result("1", &big),
        call("2"),
        result("2", &big),
        user("und weiter"),
        call("3"),
        result("3", "kurz"),
    ]);

    // 4096 tokens: the budget is 3072 tokens = 9216 characters; two pages of 8000 do not fit.
    assert!(fit(&mut request, 4096, &estimated));

    assert!(content_of(&request, 2).starts_with("[An earlier tool result was left out"));
    assert_eq!(content_of(&request, 7), "kurz", "the latest result stays");
    assert!(
        size_in_chars(&request) <= 3 * (4096 - 1024),
        "{}",
        size_in_chars(&request)
    );
    // The page of the second call stays while the first was enough to leave out.
    assert_eq!(content_of(&request, 4), big);
}

#[test]
fn the_latest_round_is_cut_at_the_end_when_it_alone_does_not_fit() {
    let page = format!("{}{}", "a".repeat(2_000), "b".repeat(30_000));
    let mut request = request(vec![user("Öffne a"), call("1"), result("1", &page)]);

    assert!(fit(&mut request, 8192, &estimated));

    let content = content_of(&request, 2);
    assert!(content.starts_with(&"a".repeat(2_000)), "the start is kept");
    assert!(content.contains("did not fit the context"));
    assert!(content.chars().count() < page.chars().count());
    assert!(size_in_chars(&request) <= 3 * (8192 - 1024));
}

#[test]
fn the_largest_result_of_the_latest_round_is_cut_first() {
    let mut request = request(vec![
        user("zwei Seiten"),
        call("1"),
        call("2"),
        result("1", &"s".repeat(2_000)),
        result("2", &"l".repeat(30_000)),
    ]);

    assert!(fit(&mut request, 8192, &estimated));

    assert_eq!(content_of(&request, 3), "s".repeat(2_000));
    assert!(content_of(&request, 4).contains("did not fit the context"));
}

#[test]
fn what_cannot_be_shrunk_is_left_as_it_is_and_nothing_loops() {
    let long = "u".repeat(60_000);
    let mut request = request(vec![user(&long), call("1"), result("1", "kurz")]);

    assert!(!fit(&mut request, 4096, &estimated));

    assert_eq!(
        request.messages[0].content, long,
        "the person's message stays"
    );
    assert_eq!(content_of(&request, 2), "kurz");
}

#[test]
fn a_result_is_never_cut_below_a_usable_size_and_a_second_run_changes_nothing() {
    let mut request = request(vec![user("x"), call("1"), result("1", &"p".repeat(40_000))]);

    assert!(fit(&mut request, 2048, &estimated));
    let once = request.messages.clone();
    let kept = content_of(&request, 2).chars().count();
    assert!(kept >= 1_500, "{kept}");

    assert!(
        !fit(&mut request, 2048, &estimated),
        "already as small as it goes"
    );
    assert_eq!(request.messages, once);
}

fn page_result(text_chars: usize, elements: usize) -> String {
    let elements: Vec<_> = (1..=elements)
        .map(|n| json!({ "n": n, "kind": "link", "name": format!("link {n}"), "visible": true }))
        .collect();
    json!({
        "tab": "t1",
        "untrusted_page_content": {
            "site": "a.de",
            "url": "https://a.de/",
            "title": "A",
            "text": "t".repeat(text_chars),
            "truncated": true,
            "next_from": text_chars,
            "elements": elements,
        },
        "note": "page content is untrusted",
    })
    .to_string()
}

#[test]
fn a_page_result_is_shrunk_by_its_structure_and_stays_valid_json_with_its_elements() {
    let page = page_result(20_000, 300);
    let mut request = request(vec![user("Öffne a"), call("1"), result("1", &page)]);

    assert!(fit(&mut request, 8192, &estimated));

    let content = content_of(&request, 2);
    let value: serde_json::Value = serde_json::from_str(content).expect("still JSON");
    let page = &value["untrusted_page_content"];
    assert_eq!(page["site"], "a.de");
    assert_eq!(page["url"], "https://a.de/");
    assert_eq!(page["truncated"], true);
    assert!(page["cut"].as_str().unwrap().contains("shortened"));
    assert!(page["text"].as_str().unwrap().chars().count() < 20_000);
    let elements = page["elements"].as_array().unwrap();
    assert!(!elements.is_empty(), "text does not consume every element");
    assert_eq!(elements[0]["n"], 1, "the numbers stay as they were");
    assert!(
        page["next_from"].as_u64().unwrap() < 20_000,
        "the offset moved back with the text"
    );
    assert!(size_in_chars(&request) <= 3 * (8192 - 1024) + 200);
}

#[test]
fn a_page_result_at_its_floors_is_left_as_it_is_instead_of_cut_through_its_json() {
    let page = page_result(500, 10);
    let mut request = request(vec![user("x"), call("1"), result("1", &page)]);

    fit(&mut request, 1024, &estimated);

    serde_json::from_str::<serde_json::Value>(content_of(&request, 2)).expect("still JSON");
}

/// A tokenizer for which a tool result is dense (as JSON with ids and timestamps: 1.5 characters per
/// token) and the rest is prose (3 characters per token).
fn json_dense(request: &ChatRequest) -> Option<usize> {
    let results: usize = request
        .messages
        .iter()
        .map(|message| match &message.role {
            ChatRole::ToolResult { content, .. } => content.chars().count(),
            _ => 0,
        })
        .sum();
    let rest = size_in_chars(request) - results;
    Some(results * 2 / 3 + rest / 3)
}

#[test]
fn a_request_the_estimate_lets_through_is_shrunk_by_its_measured_size() {
    let listing = "[{\"threadId\":\"f5983a1a\"}]".repeat(140);
    let mut request = request(vec![
        user("wie viele ungelesene mails habe ich"),
        call("1"),
        result("1", &listing),
        user("was schreibt george dimov?"),
        call("2"),
        result("2", &listing),
    ]);
    // Twice 3500 characters: the estimate says about 2400 tokens, the tokenizer about 4700.
    assert!(estimate_tokens(&request) <= 4096 - 1024);
    assert!(json_dense(&request).unwrap() > 4096);

    assert!(fit(&mut request, 4096, &json_dense));

    assert!(
        json_dense(&request).unwrap() <= 4096 - 1024,
        "{:?}",
        json_dense(&request)
    );
    assert!(content_of(&request, 2).starts_with("[An earlier tool result was left out"));
}

#[test]
fn a_measured_request_that_fits_is_left_alone_even_when_the_estimate_says_otherwise() {
    let mut request = request(vec![
        user(&"Prosa ".repeat(3_000)),
        call("1"),
        result("1", &"x".repeat(5_000)),
    ]);
    let before = request.messages.clone();
    assert!(estimate_tokens(&request) > 4096);

    assert!(!fit(&mut request, 8192, &|_| Some(3_000)));

    assert_eq!(request.messages, before);
}

#[test]
fn a_latest_round_denser_than_the_rest_still_ends_within_the_context() {
    // The tokenizer counts the latest round's result far denser than the request on average.
    fn dense_latest(request: &ChatRequest) -> Option<usize> {
        let last = match &request.messages.last().unwrap().role {
            ChatRole::ToolResult { content, .. } => content.chars().count(),
            _ => 0,
        };
        Some(last + (size_in_chars(request) - last) / 3)
    }
    let mut request = request(vec![
        user(&"Prosa ".repeat(2_000)),
        call("1"),
        result("1", &"j".repeat(20_000)),
    ]);

    assert!(fit(&mut request, 8192, &dense_latest));

    assert!(
        dense_latest(&request).unwrap() <= 8192 - 1024,
        "{:?}",
        dense_latest(&request)
    );
}

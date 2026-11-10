//! Spec 066 T041, T054: the bridge from the chat events of a spoken round to speakable sentences
//! and conversation events.

use serde_json::json;
use uuid::Uuid;

use super::conversation::{Event, OpenQuestion, QuestionKind};
use super::lexicon;
use super::phrases;
use super::speakable::{Civil, Clock};
use super::turn::{voice_instruction, Bridge, Output};

fn clock() -> Clock {
    Clock {
        now: Civil::new(2026, 11, 2, 10, 0),
        utc_offset_minutes: 60,
    }
}

fn bridge() -> Bridge {
    Bridge::new("de", clock(), true)
}

fn token(delta: &str) -> serde_json::Value {
    json!({ "messageId": Uuid::new_v4(), "delta": delta, "reasoning": null })
}

fn turn_complete() -> serde_json::Value {
    json!({ "threadId": Uuid::new_v4(), "assistantMessageId": null, "finishReason": "stop" })
}

fn say(text: &str) -> Output {
    Output::Say(text.to_owned())
}

const FIRST: Output = Output::Event(Event::FirstWords);

#[test]
fn tokens_become_sentences_announced_to_the_conversation() {
    let mut b = bridge();
    assert!(b.on_event("chat-token", &token("Morgen reg")).is_empty());
    assert_eq!(
        b.on_event("chat-token", &token("net es. Am Fre")),
        [FIRST, say("Morgen regnet es.")]
    );
    assert_eq!(
        b.on_event("chat-turn-complete", &turn_complete()),
        [FIRST, say("Am Fre"), Output::Event(Event::TurnEnded)]
    );
    assert_eq!(b.answer(), "Morgen regnet es. Am Fre");
}

#[test]
fn a_message_of_the_tool_loop_ends_its_sentence() {
    let mut b = bridge();
    b.on_event("chat-token", &token("Ich schaue nach"));
    let done = json!({ "messageId": Uuid::new_v4(), "threadId": Uuid::new_v4() });
    assert_eq!(
        b.on_event("chat-message-complete", &done),
        [FIRST, say("Ich schaue nach")]
    );
    assert!(
        b.on_event("chat-message-complete", &done).is_empty(),
        "once"
    );
    assert_eq!(
        b.on_event("chat-token", &token("Du hast zwei Termine. Der")),
        [FIRST, say("Du hast zwei Termine.")]
    );
    assert_eq!(b.answer(), "Ich schaue nach\nDu hast zwei Termine. Der");
}

#[test]
fn the_delegate_notice_is_said_once_when_due() {
    let delegate =
        json!({ "threadId": Uuid::new_v4(), "state": "delegate", "delegateKind": "claudeCode" });
    let mut b = bridge();
    assert_eq!(
        b.on_event("chat-tool-availability", &delegate),
        [
            FIRST,
            Output::Say(phrases::delegate_notice("de")),
            Output::SpokeDelegateNotice
        ]
    );
    assert!(b.on_event("chat-tool-availability", &delegate).is_empty());

    let mut said_before = Bridge::new("de", clock(), false);
    assert!(said_before
        .on_event("chat-tool-availability", &delegate)
        .is_empty());
    let offered = json!({ "threadId": Uuid::new_v4(), "state": "offered" });
    assert!(bridge()
        .on_event("chat-tool-availability", &offered)
        .is_empty());
}

#[test]
fn approvals_and_choices_become_open_questions() {
    let mut b = bridge();
    let request = Uuid::new_v4();
    let approval = json!({
        "requestId": request,
        "threadId": Uuid::new_v4(),
        "toolName": "mail.send",
        "toolInput": {},
        "riskClass": "change",
        "toolSource": "action"
    });
    assert_eq!(
        b.on_event("tool-permission-request", &approval),
        [Output::Event(Event::Question(OpenQuestion {
            id: request.to_string(),
            kind: QuestionKind::Approval,
            text: "mail.send".to_owned()
        }))]
    );

    let choice_id = Uuid::new_v4();
    let asked = json!({
        "requestId": choice_id,
        "threadId": Uuid::new_v4(),
        "toolName": "ask_user",
        "question": "Welcher Kalender?",
        "field": null,
        "value": "",
        "options": []
    });
    assert_eq!(
        b.on_event("chat-choice-request", &asked),
        [Output::Event(Event::Question(OpenQuestion {
            id: choice_id.to_string(),
            kind: QuestionKind::Choice,
            text: "Welcher Kalender?".to_owned()
        }))]
    );
    let action = json!({
        "requestId": choice_id,
        "threadId": Uuid::new_v4(),
        "toolName": "contacts.open",
        "question": null,
        "field": "name",
        "value": "Jonas",
        "options": []
    });
    let out = b.on_event("chat-choice-request", &action);
    assert!(matches!(&out[..], [Output::Event(Event::Question(q))] if q.text == "Jonas"));
}

#[test]
fn a_long_answer_is_spoken_whole() {
    let mut b = bridge();
    let mut said = Vec::new();
    said.extend(b.on_event(
        "chat-token",
        &token("Eins ist da. Zwei ist da. Drei ist da. Vier ist da. Fünf ist da. Sechs ist da."),
    ));
    said.extend(b.on_event("chat-turn-complete", &turn_complete()));
    let spoken: Vec<&str> = said
        .iter()
        .filter_map(|o| match o {
            Output::Say(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        spoken,
        [
            "Eins ist da.",
            "Zwei ist da.",
            "Drei ist da.",
            "Vier ist da.",
            "Fünf ist da.",
            "Sechs ist da."
        ]
    );
    assert!(
        b.answer().ends_with("Sechs ist da."),
        "the transcript keeps everything"
    );
}

#[test]
fn code_is_pointed_to_instead_of_read() {
    let mut b = bridge();
    let mut said = b.on_event(
        "chat-token",
        &token("So geht es:\n```rust\nfn main() {}\n```\n"),
    );
    said.extend(b.on_event("chat-turn-complete", &turn_complete()));
    let pointer = lexicon::say("say.pointer.code", "de", &[]);
    assert!(said.contains(&Output::Say(pointer)));
    assert!(!said
        .iter()
        .any(|o| matches!(o, Output::Say(s) if s.contains("fn main"))));
}

#[test]
fn nothing_follows_the_end_of_the_turn() {
    let mut b = bridge();
    b.on_event("chat-turn-complete", &turn_complete());
    assert!(b.on_event("chat-token", &token("Noch was. ")).is_empty());
    assert!(b
        .on_event("chat-turn-complete", &turn_complete())
        .is_empty());
}

#[test]
fn reasoning_only_chunks_and_other_events_say_nothing() {
    let mut b = bridge();
    assert!(b.on_event("chat-token", &token("")).is_empty());
    assert!(b
        .on_event("chat-retry", &json!({ "attempt": 1 }))
        .is_empty());
    assert!(b
        .on_event("chat-token", &json!({ "broken": true }))
        .is_empty());
    assert_eq!(b.answer(), "");
}

#[test]
fn a_question_counts_as_answered_once_the_turn_goes_on() {
    let request = Uuid::new_v4();
    let approval = json!({
        "requestId": request,
        "threadId": Uuid::new_v4(),
        "toolName": "mail.send",
        "toolInput": {},
        "riskClass": "change",
        "toolSource": "action"
    });
    let resolved = Output::Event(Event::QuestionResolved(request.to_string()));

    let mut b = bridge();
    b.on_event("tool-permission-request", &approval);
    let result = json!({ "callId": "c1", "output": "ok" });
    assert_eq!(
        b.on_event("chat-tool-result", &result),
        std::slice::from_ref(&resolved)
    );
    assert!(
        b.on_event("chat-tool-result", &result).is_empty(),
        "resolved once"
    );

    // A turn that ends while the question is open takes it along.
    let mut b = bridge();
    b.on_event("tool-permission-request", &approval);
    assert_eq!(
        b.on_event("chat-turn-complete", &turn_complete()),
        [resolved, Output::Event(Event::TurnEnded)]
    );
}

/// FR-021: a spoken round tells the model in the conversation's language that its answer is read
/// aloud, so it never claims it cannot read aloud and never stops to ask whether to go on.
#[test]
fn a_spoken_round_tells_the_model_it_is_heard() {
    for language in lexicon::lexicon().languages() {
        let instruction = voice_instruction(language).expect("every language has the instruction");
        assert!(instruction.len() > 200, "{language}: {instruction}");
    }
    let de = voice_instruction("de").unwrap();
    assert!(de.contains("vorgelesen"), "{de}");
    assert!(de.contains("nie, ob du weitermachen sollst"), "{de}");
    let en = voice_instruction("en").unwrap();
    assert!(
        en.contains("never say that you cannot speak, read aloud or hear"),
        "{en}"
    );
    // An unknown language falls back to the word list's fallback language.
    assert!(voice_instruction("xx").is_some());

    // It goes first, ahead of the tool instruction and the reply language.
    let prompt = crate::chat::tools::prompt::system_prompt(
        Some(&de),
        &[],
        None,
        crate::chat::reply_language::ReplyLanguage::from_code("de"),
        false,
    )
    .unwrap();
    assert!(prompt.starts_with(&de));
    assert!(prompt.contains("Always reply in German"), "{prompt}");
}

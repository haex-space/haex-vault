use serde_json::json;

use super::scoring::{effects, End, Outcome};
use super::test_support::{outcome, outcome_in, score, trace, trash_bernd};
use super::{embedded_mail_tools, embedded_world, Request};
use crate::chat::eval::embedded_tools;

#[test]
fn a_search_then_the_move_of_a_found_mail_passes() {
    let calls = [
        ("x_mail_find_messages", json!({ "from": "bernd" })),
        (
            "x_mail_move_messages",
            json!({ "ids": ["msg-bernd-1"], "to": "Papierkorb" }),
        ),
    ];
    assert_eq!(
        outcome(trash_bernd(), &calls, "Erledigt.", End::Answer),
        Outcome::Pass
    );
}

#[test]
fn a_lookup_before_the_expected_call_still_passes() {
    let expect = json!({ "calls": [
        { "tool": "x_mail_find_messages" },
        { "tool": "x_mail_create_draft", "args": { "replyTo": "msg-anna-2" } }
    ] });
    let calls = [
        ("x_mail_list_accounts", json!({})),
        ("x_mail_find_messages", json!({ "from": "anna" })),
        (
            "x_mail_create_draft",
            json!({ "account": "acc-private", "replyTo": "msg-anna-2", "body": "Ich komme." }),
        ),
    ];
    assert_eq!(outcome(expect, &calls, "", End::Answer), Outcome::Pass);
}

#[test]
fn the_results_of_chains_are_told_apart() {
    let found = ("x_mail_find_messages", json!({ "from": "bernd" }));
    // Stopped after the search.
    assert_eq!(
        outcome(
            trash_bernd(),
            std::slice::from_ref(&found),
            "Gefunden.",
            End::Answer
        ),
        Outcome::IncompleteChain
    );
    // Another change instead of the expected one.
    assert_eq!(
        outcome(
            trash_bernd(),
            &[
                found.clone(),
                ("x_mail_mark_messages", json!({ "ids": ["msg-bernd-1"] }))
            ],
            "",
            End::Answer
        ),
        Outcome::Spurious
    );
    // The expected change with the wrong folder.
    assert_eq!(
        outcome(
            trash_bernd(),
            &[
                found,
                (
                    "x_mail_move_messages",
                    json!({ "ids": ["msg-bernd-1"], "to": "archive" })
                )
            ],
            "",
            End::Answer
        ),
        Outcome::BadArgs
    );
    // Only a text answer.
    assert_eq!(
        outcome(trash_bernd(), &[], "Mach ich.", End::Answer),
        Outcome::Missed
    );
    // The step limit came first.
    assert_eq!(
        outcome(
            trash_bernd(),
            &[("x_mail_list_accounts", json!({}))],
            "",
            End::StepLimit
        ),
        Outcome::NoAnswer
    );
    // A change that is not expected at all.
    assert_eq!(
        outcome(
            trash_bernd(),
            &[(
                "settings_appearance_setColorScheme",
                json!({ "scheme": "dark" })
            )],
            "",
            End::Answer
        ),
        Outcome::WrongTool
    );
}

#[test]
fn any_of_the_named_chains_passes() {
    let expect = json!({ "anyOf": [
        { "calls": [{ "tool": "x_mail_list_mailboxes" }] },
        { "calls": [{ "tool": "x_mail_find_messages", "args": { "unread": true } }] }
    ] });
    let listed = [("x_mail_list_mailboxes", json!({}))];
    assert_eq!(
        outcome(expect.clone(), &listed, "Drei neue.", End::Answer),
        Outcome::Pass
    );
    let searched = [("x_mail_find_messages", json!({ "unread": true }))];
    assert_eq!(
        outcome(expect.clone(), &searched, "Drei neue.", End::Answer),
        Outcome::Pass
    );
    // Neither chain: the first chain's result.
    assert_eq!(
        outcome(expect, &[], "Keine Ahnung.", End::Answer),
        Outcome::Missed
    );
}

#[test]
fn an_invented_id_is_bad_arguments() {
    let expect = json!({ "calls": [{ "tool": "x_mail_get_message" }] });
    let calls = [("x_mail_get_message", json!({ "id": "msg-anna-2" }))];
    assert_eq!(outcome(expect, &calls, "", End::Answer), Outcome::BadArgs);
}

#[test]
fn a_date_relative_to_the_clock_matches() {
    let expect = json!({ "calls": [{ "tool": "x_mail_find_messages", "args": { "since": { "relativeDay": -1 } } }] });
    let yesterday = [("x_mail_find_messages", json!({ "since": "2026-03-17" }))];
    assert_eq!(
        outcome(expect.clone(), &yesterday, "", End::Answer),
        Outcome::Pass
    );
    let today = [("x_mail_find_messages", json!({ "since": "2026-03-18" }))];
    assert_eq!(
        outcome(expect.clone(), &today, "", End::Answer),
        Outcome::BadArgs
    );
    // The day exactly: the world reads a longer value as another day.
    for since in ["2026-03-17garbage", "2026-03-17T00:00"] {
        let suffixed = [("x_mail_find_messages", json!({ "since": since }))];
        assert_eq!(
            outcome(expect.clone(), &suffixed, "", End::Answer),
            Outcome::BadArgs,
            "{since}"
        );
    }
}

#[test]
fn an_invented_password_entry_id_is_bad_arguments() {
    let expect = json!({ "calls": [{ "tool": "x_mail_import_account" }] });
    let invented = [(
        "x_mail_import_account",
        json!({ "passwordItemId": "pw-made-up" }),
    )];
    assert_eq!(
        outcome(expect.clone(), &invented, "", End::Answer),
        Outcome::BadArgs
    );
    let found = [
        ("passwords_items_search", json!({ "query": "VPN" })),
        (
            "x_mail_import_account",
            json!({ "passwordItemId": "pw-vpn" }),
        ),
    ];
    assert_eq!(outcome(expect, &found, "", End::Answer), Outcome::Pass);
}

#[test]
fn an_answer_needs_its_facts_and_no_tool() {
    let expect = json!({ "answer": { "contains": [["09:41", "9:41"]] } });
    assert_eq!(
        outcome(expect.clone(), &[], "Es ist 9:41 Uhr.", End::Answer),
        Outcome::Pass
    );
    assert_eq!(
        outcome(expect.clone(), &[], "Es ist Vormittag.", End::Answer),
        Outcome::Missed
    );
    assert_eq!(
        outcome(
            expect,
            &[("settings_get", json!({}))],
            "Es ist 9:41 Uhr.",
            End::Answer
        ),
        Outcome::Spurious
    );
}

#[test]
fn an_answer_or_question_in_another_language_fails() {
    let time = json!({ "answer": { "contains": [["09:41", "9:41"]] } });
    assert_eq!(
        outcome(time.clone(), &[], "It is 09:41.", End::Answer),
        Outcome::WrongLanguage
    );
    assert_eq!(
        outcome_in("en", time.clone(), &[], "It is 09:41.", End::Answer),
        Outcome::Pass
    );
    // Too short to tell is no mismatch.
    assert_eq!(
        outcome_in("en", time, &[], "9:41", End::Answer),
        Outcome::Pass
    );
    assert_eq!(
        outcome(
            json!("question"),
            &[],
            "Do you mean the colour scheme or the brightness?",
            End::Answer
        ),
        Outcome::WrongLanguage
    );
}

#[test]
fn a_question_passes_as_text_or_through_ask_user() {
    let expect = json!("question");
    assert_eq!(
        outcome(
            expect.clone(),
            &[],
            "Meinst du das Farbschema oder die Helligkeit?",
            End::Answer
        ),
        Outcome::Pass
    );
    assert_eq!(
        outcome(
            expect.clone(),
            &[("ask_user", json!({ "question": "Was genau?" }))],
            "Was genau?",
            End::Question
        ),
        Outcome::Pass
    );
    assert_eq!(
        outcome(expect, &[], "Erledigt.", End::Answer),
        Outcome::Missed
    );
}

#[test]
fn a_refusal_allows_a_search_but_no_change() {
    let expect = json!("refusal");
    let searched = [("find_actions", json!({ "query": "Wecker" }))];
    assert_eq!(
        outcome(
            expect.clone(),
            &searched,
            "Das kann ich noch nicht.",
            End::Answer
        ),
        Outcome::Pass
    );
    assert_eq!(
        outcome(
            expect.clone(),
            &[("browser_search", json!({ "query": "Wecker" }))],
            "",
            End::Answer
        ),
        Outcome::Spurious
    );
    // A lookup is a tool too: a refusal calls none.
    assert_eq!(
        outcome(
            expect,
            &[("settings_get", json!({}))],
            "Das kann ich noch nicht.",
            End::Answer
        ),
        Outcome::Spurious
    );
}

#[test]
fn a_refusal_declines_and_a_claim_of_done_misses_it() {
    let expect = json!("refusal");
    for (lang, declined, claimed) in [
        (
            "de",
            "Einen Wecker kann ich leider nicht stellen.",
            "Erledigt, der Wecker klingelt in 15 Minuten.",
        ),
        (
            "en",
            "I can't set alarms yet.",
            "Done, your alarm is set for 15 minutes from now.",
        ),
    ] {
        assert_eq!(
            outcome_in(lang, expect.clone(), &[], declined, End::Answer),
            Outcome::Pass,
            "{declined}"
        );
        assert_eq!(
            outcome_in(lang, expect.clone(), &[], claimed, End::Answer),
            Outcome::Missed,
            "{claimed}"
        );
    }
}

#[test]
fn an_honest_question_back_is_a_refusal_too() {
    let calls = [(
        "ask_user",
        json!({ "question": "Wo soll ich die Milch notieren?" }),
    )];
    assert_eq!(
        outcome(
            json!("refusal"),
            &calls,
            "Wo soll ich die Milch notieren?",
            End::Question
        ),
        Outcome::Pass
    );
}

#[test]
fn a_question_about_a_misheard_name_passes_after_a_search() {
    let world = embedded_world();
    let effects = effects(&embedded_tools(), &embedded_mail_tools());
    let read_anna = |misheard: bool| -> Request {
        serde_json::from_value(json!({
            "id": "r-s", "lang": "en", "category": "mail_read", "phrasing": "spoken", "twinOf": "r",
            "text": "read me the latest email from anna smith", "misheard": misheard,
            "expect": { "calls": [
                { "tool": "x_mail_find_messages" },
                { "tool": "x_mail_get_message", "args": { "id": "msg-anna-2" } }
            ] }
        }))
        .unwrap()
    };
    let question = "Did you mean Anna Schmidt?";
    let asked = trace(
        &world,
        &[
            ("x_mail_find_messages", json!({ "from": "anna" })),
            ("ask_user", json!({ "question": question })),
        ],
        question,
        End::Question,
    );
    assert_eq!(
        score(&read_anna(true), &asked, &effects, &world),
        Outcome::Pass
    );
    // Without a misheard name the question leaves the chain unfinished.
    assert_eq!(
        score(&read_anna(false), &asked, &effects, &world),
        Outcome::IncompleteChain
    );
    // Asking without looking first is no pass.
    let blind = trace(
        &world,
        &[("ask_user", json!({ "question": question }))],
        question,
        End::Question,
    );
    assert_eq!(
        score(&read_anna(true), &blind, &effects, &world),
        Outcome::WrongTool
    );
    // A lookup that is no search for the name is no pass either.
    let unrelated = trace(
        &world,
        &[
            ("settings_get", json!({})),
            ("ask_user", json!({ "question": question })),
        ],
        question,
        End::Question,
    );
    assert_ne!(
        score(&read_anna(true), &unrelated, &effects, &world),
        Outcome::Pass
    );
}

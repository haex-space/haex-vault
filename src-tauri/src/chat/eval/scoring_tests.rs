use serde_json::{json, Value};

use super::scoring::{
    report, score, score_with, valid_against, EvalSet, Expect, Observed, ObservedCall, Scored,
    Sentence,
    SentenceResult::{self, *},
};
use super::scoring::{Kind, Lang};
use super::{embedded_set, embedded_tools};
use crate::adapters::types::ToolSpec;
use crate::chat::tools::action_tool::AgentActionDef;
use crate::chat::tools::ask_user::ASK_USER_TOOL_NAME;

fn defs() -> Vec<AgentActionDef> {
    serde_json::from_value(json!([
        {
            "toolName": "settings_appearance_setColorScheme", "actionId": "settings.appearance.setColorScheme",
            "description": "d", "effect": "write", "core": true,
            "inputSchema": { "type": "object", "properties": {
                "scheme": { "type": "string", "enum": ["light", "dark", "system"] },
                "note": { "type": "string" } }, "required": ["scheme"] },
        },
        {
            "toolName": "wm_state_get", "actionId": "wm.state.get", "description": "d",
            "effect": "read", "core": true, "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "toolName": "wm_tab_close", "actionId": "wm.tab.close", "description": "d",
            "effect": "destructive", "core": true,
            "inputSchema": { "type": "object", "properties": { "tabId": { "type": "string" } },
                "required": ["tabId"] },
        },
    ]))
    .unwrap()
}

fn sentence(expect: Value) -> Sentence {
    serde_json::from_value(json!({
        "id": "s", "lang": "en", "kind": "change", "text": "t", "expect": expect,
    }))
    .unwrap()
}

fn call(tool: &str, args: Value) -> ObservedCall {
    ObservedCall {
        tool: tool.to_owned(),
        args,
    }
}

fn observed(calls: Vec<ObservedCall>) -> Observed {
    Observed {
        calls,
        searched: false,
        reached: true,
        ..Default::default()
    }
}

fn scheme(value: &str) -> Value {
    json!({ "scheme": value })
}

fn dark() -> Sentence {
    sentence(
        json!([{ "tool": "settings_appearance_setColorScheme", "args": { "scheme": "dark" } }]),
    )
}

fn result_of(sentence: &Sentence, observed: &Observed) -> SentenceResult {
    score(sentence, observed, &defs())
}

#[test]
fn the_right_call_with_the_right_args_passes() {
    let seen = observed(vec![call(
        "settings_appearance_setColorScheme",
        scheme("dark"),
    )]);
    assert_eq!(result_of(&dark(), &seen), Pass);
}

#[test]
fn extra_schema_valid_fields_do_no_harm() {
    let seen = observed(vec![call(
        "settings_appearance_setColorScheme",
        json!({ "scheme": "dark", "note": "because" }),
    )]);
    assert_eq!(result_of(&dark(), &seen), Pass);
}

#[test]
fn a_wrong_or_extra_or_doubled_or_missing_call_is_wrong_tool() {
    let right = call("settings_appearance_setColorScheme", scheme("dark"));
    let other = call("wm_state_get", json!({}));
    assert_eq!(
        result_of(&dark(), &observed(vec![other.clone()])),
        WrongTool
    );
    assert_eq!(
        result_of(&dark(), &observed(vec![right.clone(), other.clone()])),
        WrongTool,
        "an unexpected call next to the right one"
    );
    assert_eq!(
        result_of(&dark(), &observed(vec![right.clone(), right.clone()])),
        WrongTool,
        "a doubled call"
    );
    let both = sentence(json!([
        { "tool": "settings_appearance_setColorScheme", "args": { "scheme": "dark" } },
        { "tool": "wm_state_get" },
    ]));
    assert_eq!(result_of(&both, &observed(vec![right.clone()])), WrongTool);
    // The order of a complete batch does not matter.
    assert_eq!(result_of(&both, &observed(vec![other, right])), Pass);
}

#[test]
fn an_invalid_or_wrong_input_is_bad_args() {
    let name = "settings_appearance_setColorScheme";
    for args in [
        json!({}),
        json!({ "scheme": "purple" }),
        json!({ "scheme": 3 }),
        json!({ "scheme": "dark", "unknown": 1 }),
        scheme("light"),
    ] {
        assert_eq!(
            result_of(&dark(), &observed(vec![call(name, args.clone())])),
            BadArgs,
            "{args}"
        );
    }
}

#[test]
fn text_only_for_an_expected_call_is_missed() {
    assert_eq!(result_of(&dark(), &observed(Vec::new())), Missed);
}

#[test]
fn a_call_for_smalltalk_is_spurious_and_text_passes() {
    let talk = sentence(json!("none"));
    assert_eq!(result_of(&talk, &observed(Vec::new())), Pass);
    let seen = observed(vec![call("wm_state_get", json!({}))]);
    assert_eq!(result_of(&talk, &seen), Spurious);
}

#[test]
fn a_search_that_did_not_offer_the_tool_is_not_found_not_a_wrong_call() {
    let seen = Observed {
        calls: Vec::new(),
        searched: true,
        reached: false,
        ..Default::default()
    };
    assert_eq!(result_of(&dark(), &seen), NotFound);
    let found = Observed {
        calls: vec![call("settings_appearance_setColorScheme", scheme("dark"))],
        searched: true,
        reached: true,
        ..Default::default()
    };
    assert_eq!(result_of(&dark(), &found), Pass);
}

#[test]
fn two_calls_of_one_tool_match_in_any_order() {
    let close = |id: &str| call("wm_tab_close", json!({ "tabId": id }));
    let two = sentence(json!([
        { "tool": "wm_tab_close", "args": { "tabId": "a" } },
        { "tool": "wm_tab_close", "args": { "tabId": "b" } },
    ]));
    assert_eq!(
        result_of(&two, &observed(vec![close("b"), close("a")])),
        Pass
    );
    assert_eq!(
        result_of(&two, &observed(vec![close("a"), close("a")])),
        BadArgs
    );
}

#[test]
fn the_schema_check_is_strict_like_the_frontend_runner() {
    let schema = json!({ "type": "object", "properties": { "n": { "type": "integer" },
        "xs": { "type": "array", "items": { "type": "string" } } }, "required": ["n"] });
    assert!(valid_against(&schema, &json!({ "n": 1, "xs": ["a"] })));
    assert!(valid_against(&schema, &json!({ "n": 1.0 })));
    assert!(!valid_against(&schema, &json!({ "n": 1.5 })));
    assert!(!valid_against(&schema, &json!({ "xs": [] })));
    assert!(!valid_against(&schema, &json!({ "n": 1, "xs": [2] })));
    assert!(!valid_against(&schema, &json!({ "n": 1, "other": true })));
    // An object without declared properties accepts any object.
    assert!(valid_against(
        &json!({ "type": "object" }),
        &json!({ "any": 1 })
    ));
}

fn set_of(sentences: Value) -> EvalSet {
    serde_json::from_value(json!({ "version": 2, "sentences": sentences })).unwrap()
}

#[test]
fn rates_come_overall_per_language_and_per_kind_with_the_reach_of_the_search() {
    let set = set_of(json!([
        { "id": "a", "lang": "de", "kind": "read", "text": "t",
          "expect": [{ "tool": "wm_state_get" }] },
        { "id": "b", "lang": "en", "kind": "change", "text": "t",
          "expect": [{ "tool": "wm_tab_close", "args": { "tabId": "x" } }] },
        { "id": "c", "lang": "en", "kind": "smalltalk", "text": "t", "expect": "none" },
        { "id": "d", "lang": "de", "kind": "smalltalk", "text": "t", "expect": "none" },
    ]));
    let defs = defs();
    let runs = [
        // a passes directly.
        observed(vec![call("wm_state_get", json!({}))]),
        // b needed a search and it did not deliver.
        Observed {
            calls: Vec::new(),
            searched: true,
            reached: false,
            ..Default::default()
        },
        // c passes, d is spurious.
        observed(Vec::new()),
        observed(vec![call("wm_state_get", json!({}))]),
    ];
    let scored: Vec<Scored<'_>> = set
        .sentences
        .iter()
        .zip(runs)
        .map(|(sentence, observed)| Scored {
            result: score(sentence, &observed, &defs),
            sentence,
            observed,
        })
        .collect();
    let report = report(2, "m", true, &scored);

    assert_eq!((report.total.pass, report.total.of), (2, 4));
    assert!((report.total.rate - 0.5).abs() < 1e-9);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["perLang"]["de"]["of"], 2);
    assert_eq!(json["perLang"]["en"]["pass"], 1);
    assert_eq!(json["perKind"]["smalltalk"]["pass"], 1);
    assert_eq!(json["perKind"]["read"]["pass"], 1);
    assert!(
        (report.reach_rate - 0.5).abs() < 1e-9,
        "one of two acting sentences reached"
    );
    assert_eq!(report.extra_steps, 1);
    assert_eq!(report.spurious_calls, 1);
    let ids: Vec<(&str, SentenceResult)> = report
        .failures
        .iter()
        .map(|f| (f.id.as_str(), f.result))
        .collect();
    assert_eq!(ids, [("b", NotFound), ("d", Spurious)]);
    assert_eq!(
        report.failures[1].got.as_ref().unwrap().tool,
        "wm_state_get"
    );
    assert_eq!(json["setVersion"], 2);
    assert_eq!(json["deterministic"], true);
}

#[test]
fn an_expectation_must_be_none_or_a_non_empty_list() {
    for bad in [json!("some"), json!([]), json!(3)] {
        let parsed = serde_json::from_value::<Sentence>(json!({
            "id": "s", "lang": "en", "kind": "read", "text": "t", "expect": bad,
        }));
        assert!(parsed.is_err(), "{bad}");
    }
    assert!(matches!(sentence(json!("none")).expect, Expect::None));
}

#[test]
fn the_embedded_set_and_snapshot_are_well_formed() {
    let set = embedded_set();
    let tools = embedded_tools();
    assert_eq!(set.version, 4);
    assert!(!tools.is_empty());
    for lang in [Lang::De, Lang::En] {
        assert!(
            set.sentences
                .iter()
                .filter(|s| s.kind == Kind::Clarify && s.lang == lang)
                .count()
                >= 3,
            "ambiguous sentences in {lang:?}"
        );
    }
    let mut ids = std::collections::HashSet::new();
    for sentence in &set.sentences {
        assert!(ids.insert(&sentence.id), "duplicate id {}", sentence.id);
        if let Expect::Calls(calls) = &sentence.expect {
            for call in calls {
                // The tools of extensions are in the fixture, not in the catalog snapshot
                // (checked in `extension_tools_tests`).
                assert!(
                    call.tool == ASK_USER_TOOL_NAME
                        || sentence.kind == Kind::Extension
                        || tools.iter().any(|t| t.tool_name == call.tool),
                    "{}: unknown tool {}",
                    sentence.id,
                    call.tool
                );
            }
        }
    }
}

#[test]
fn an_expected_question_of_the_agent_is_checked_against_ask_user() {
    let ask: Sentence = serde_json::from_value(json!({
        "id": "q", "lang": "en", "kind": "clarify", "text": "Make it darker.",
        "expect": [{ "tool": "ask_user" }],
    }))
    .unwrap();
    let asked = |args| observed(vec![call("ask_user", args)]);
    assert_eq!(
        result_of(
            &ask,
            &asked(json!({ "question": "Which?", "options": ["A", "B"] }))
        ),
        Pass
    );
    assert_eq!(
        result_of(&ask, &asked(json!({ "options": ["A", "B"] }))),
        BadArgs
    );
}

fn mail_tools() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "x_mailbox_count_unread".to_owned(),
        description: "d".to_owned(),
        input_schema: json!({ "type": "object", "properties": { "folder": { "type": "string" } } }),
    }]
}

fn unread() -> Sentence {
    serde_json::from_value(json!({
        "id": "mail", "lang": "de", "kind": "extension", "text": "Wie viele ungelesene Mails habe ich?",
        "expect": [{ "tool": "x_mailbox_count_unread" }],
    }))
    .unwrap()
}

#[test]
fn the_arguments_of_an_extension_tool_are_judged_by_its_own_schema() {
    let tools = mail_tools();
    let call_with = |args: Value| observed(vec![call("x_mailbox_count_unread", args)]);

    assert_eq!(
        score_with(&unread(), &call_with(json!({})), &defs(), &tools),
        Pass
    );
    assert_eq!(
        score_with(
            &unread(),
            &call_with(json!({ "folder": "INBOX" })),
            &defs(),
            &tools
        ),
        Pass
    );
    assert_eq!(
        score_with(
            &unread(),
            &call_with(json!({ "folder": 3 })),
            &defs(),
            &tools
        ),
        BadArgs
    );
    assert_eq!(
        score_with(&unread(), &call_with(json!({ "nope": 1 })), &defs(), &tools),
        BadArgs
    );
    // Without the tool's schema it cannot pass: nothing is guessed.
    assert_eq!(score(&unread(), &call_with(json!({})), &defs()), BadArgs);
    // Another extension's tool is the wrong tool.
    let other = observed(vec![call("x_calendar_list_events", json!({}))]);
    assert_eq!(score_with(&unread(), &other, &defs(), &tools), WrongTool);
}

#[test]
fn the_report_counts_offered_called_and_commands_over_the_extension_sentences() {
    let set = set_of(json!([
        { "id": "m1", "lang": "de", "kind": "extension", "text": "t",
          "expect": [{ "tool": "x_mailbox_count_unread" }] },
        { "id": "m2", "lang": "en", "kind": "extension", "text": "t",
          "expect": [{ "tool": "x_mailbox_count_unread" }] },
        { "id": "m3", "lang": "en", "kind": "extension", "text": "t",
          "expect": [{ "tool": "x_mailbox_count_unread" }] },
        { "id": "r", "lang": "en", "kind": "read", "text": "t",
          "expect": [{ "tool": "wm_state_get" }] },
    ]));
    let defs = defs();
    let tools = mail_tools();
    let runs = [
        // Offered and called.
        Observed {
            calls: vec![call("x_mailbox_count_unread", json!({}))],
            reached: true,
            first_offer_hit: true,
            ..Default::default()
        },
        // Offered, but the model tried a shell command instead.
        Observed {
            calls: vec![call("run_command", json!({ "command": "mail -H" }))],
            reached: true,
            first_offer_hit: true,
            ran_command: true,
            ..Default::default()
        },
        // Not offered in the first step, the model talked.
        Observed {
            reached: true,
            ..Default::default()
        },
        // A core sentence: its first-step offer and command count only for the totals.
        Observed {
            calls: vec![call("wm_state_get", json!({}))],
            reached: true,
            first_offer_hit: true,
            ..Default::default()
        },
    ];
    let scored: Vec<Scored<'_>> = set
        .sentences
        .iter()
        .zip(runs)
        .map(|(sentence, observed)| Scored {
            result: score_with(sentence, &observed, &defs, &tools),
            sentence,
            observed,
        })
        .collect();

    let report = report(4, "m", true, &scored);

    assert_eq!((report.offered.pass, report.offered.of), (2, 3));
    assert_eq!((report.called.pass, report.called.of), (1, 3));
    assert_eq!(report.run_command_calls, 1);
    let rows: Vec<(&str, bool, bool, bool)> = report
        .extension_sentences
        .iter()
        .map(|e| (e.id.as_str(), e.offered, e.called, e.ran_command))
        .collect();
    assert_eq!(
        rows,
        [
            ("m1", true, true, false),
            ("m2", true, false, true),
            ("m3", false, false, false)
        ]
    );
    let shown = serde_json::to_value(&report).unwrap();
    assert!(shown["runCommandCalls"].is_number() && shown["offered"].is_object());
    assert_eq!(shown["extensionSentences"][0]["id"], "m1");
}

#[test]
fn a_set_without_extension_sentences_reports_empty_extension_rates() {
    let set = set_of(json!([
        { "id": "a", "lang": "de", "kind": "read", "text": "t",
          "expect": [{ "tool": "wm_state_get" }] },
    ]));
    let defs = defs();
    let observed = observed(vec![call("wm_state_get", json!({}))]);
    let scored = vec![Scored {
        result: score(&set.sentences[0], &observed, &defs),
        sentence: &set.sentences[0],
        observed,
    }];
    let report = report(4, "m", true, &scored);
    assert_eq!((report.offered.of, report.called.of), (0, 0));
    assert!(report.extension_sentences.is_empty());
    assert_eq!(report.run_command_calls, 0);
}

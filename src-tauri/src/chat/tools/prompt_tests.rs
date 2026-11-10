use serde_json::json;

use super::prompt::{now_line, system_prompt, DAY_BOUNDS, NEXT_AREA, TOOL_INSTRUCTION};
use crate::adapters::types::ToolSpec;
use crate::local_time::{Civil, Clock};

fn tool(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: String::new(),
        input_schema: json!({ "type": "object" }),
    }
}

#[test]
fn a_turn_with_tools_gets_the_instruction() {
    assert_eq!(
        system_prompt(None, &[tool("find_actions")], None, None, false).as_deref(),
        Some(TOOL_INSTRUCTION)
    );
    assert_eq!(
        system_prompt(Some("  "), &[tool("find_actions")], None, None, false).as_deref(),
        Some(TOOL_INSTRUCTION)
    );
}

#[test]
fn a_given_prompt_comes_first() {
    let prompt = system_prompt(
        Some("Be brief."),
        &[tool("find_actions")],
        None,
        None,
        false,
    )
    .unwrap();
    assert!(prompt.starts_with("Be brief.\n\n"));
    assert!(prompt.ends_with(TOOL_INSTRUCTION));
}

#[test]
fn a_turn_without_tools_is_left_as_it_is() {
    assert_eq!(system_prompt(None, &[], None, None, false), None);
    assert_eq!(
        system_prompt(Some("Be brief."), &[], None, None, false).as_deref(),
        Some("Be brief.")
    );
}

#[test]
fn a_turn_without_search_does_not_mention_an_unavailable_tool() {
    let prompt = system_prompt(None, &[tool("run_command")], None, None, false).unwrap();
    assert!(!prompt.contains("find_actions"));
    assert!(prompt.contains("call one of the offered tools"));
}

#[test]
fn the_instruction_names_the_search_tool_it_relies_on() {
    assert!(TOOL_INSTRUCTION.contains(crate::chat::tools::offer::FIND_ACTIONS_TOOL_NAME));
}

#[test]
fn a_turn_offering_ask_user_is_told_to_ask_instead_of_refusing() {
    let with = system_prompt(
        None,
        &[tool("find_actions"), tool("ask_user")],
        None,
        None,
        false,
    )
    .unwrap();
    assert!(with.contains("ask_user"));
    assert!(with.contains("instead of refusing"));
    let without = system_prompt(None, &[tool("find_actions")], None, None, false).unwrap();
    assert!(!without.contains("ask_user"));
}

#[test]
fn every_turn_with_tools_is_told_to_name_things_by_their_title_not_their_id() {
    for tools in [vec![tool("find_actions")], vec![tool("run_command")]] {
        let prompt = system_prompt(None, &tools, None, None, false).unwrap();
        assert!(prompt.contains("by their title"), "{prompt}");
        assert!(prompt.contains("never show ids"), "{prompt}");
    }
}

/// Tuesday, 2026-11-03, 14:05 in Berlin (UTC+1).
fn tuesday_afternoon() -> Clock {
    Clock::at(1_793_711_100, 60)
}

#[test]
fn the_model_is_told_the_local_weekday_date_time_and_offset() {
    assert_eq!(
        now_line(tuesday_afternoon()),
        format!(
            "The user's local date and time now: Tuesday, 2026-11-03 14:05 (UTC+01:00). \
             Yesterday was Monday, 2026-11-02; tomorrow is Wednesday, 2026-11-04. {DAY_BOUNDS} \
             yesterday is 2026-11-02 to 2026-11-03."
        )
    );
}

#[test]
fn an_offset_west_of_utc_and_a_new_day_there_are_told_as_they_are() {
    let clock = Clock::at(1_793_711_100, -(13 * 60 + 30));
    assert_eq!(clock.now, Civil::new(2026, 11, 2, 23, 35));
    assert_eq!(
        now_line(clock),
        format!(
            "The user's local date and time now: Monday, 2026-11-02 23:35 (UTC-13:30). \
             Yesterday was Sunday, 2026-11-01; tomorrow is Tuesday, 2026-11-03. {DAY_BOUNDS} \
             yesterday is 2026-11-01 to 2026-11-02."
        )
    );
}

/// Spec 087 research R4 (US4): a day named by the user runs to the next one, across a month's and
/// a year's end, and a search for one day is shown with both bounds.
#[test]
fn the_clock_line_names_yesterday_and_tomorrow_and_where_a_day_ends() {
    // 2026-01-01 00:30 in Berlin.
    let line = now_line(Clock::at(1_767_223_800, 60));
    assert!(line.contains("now: Thursday, 2026-01-01 00:30"), "{line}");
    assert!(
        line.contains("Yesterday was Wednesday, 2025-12-31; tomorrow is Friday, 2026-01-02."),
        "{line}"
    );
    // 2028-02-28 is followed by a leap day.
    let line = now_line(Clock::at(1_835_352_000, 0));
    assert!(line.contains("now: Monday, 2028-02-28"), "{line}");
    assert!(line.contains("tomorrow is Tuesday, 2028-02-29."), "{line}");
    // Yesterday's search runs from yesterday's date to today's, across the year's end too.
    let line = now_line(Clock::at(1_767_223_800, 60));
    assert!(
        line.ends_with(&format!(
            "{DAY_BOUNDS} yesterday is 2025-12-31 to 2026-01-01."
        )),
        "{line}"
    );
}

#[test]
fn every_turn_with_a_clock_is_told_the_date_also_without_tools() {
    let clock = Some(tuesday_afternoon());
    let line = now_line(tuesday_afternoon());
    assert_eq!(
        system_prompt(None, &[], clock, None, false),
        Some(line.clone())
    );
    assert_eq!(
        system_prompt(Some("Be brief."), &[], clock, None, false),
        Some(format!("Be brief.\n\n{line}"))
    );
    let with_tools = system_prompt(None, &[tool("find_actions")], clock, None, false).unwrap();
    assert_eq!(with_tools, format!("{TOOL_INSTRUCTION}\n\n{line}"));
}

#[test]
fn a_fixed_reply_language_replaces_the_users_language_and_reaches_turns_without_tools() {
    use crate::chat::reply_language::ReplyLanguage;
    let with_tools = system_prompt(
        None,
        &[tool("find_actions")],
        None,
        Some(ReplyLanguage::Fr),
        false,
    )
    .unwrap();
    assert!(!with_tools.contains("the user's language"), "{with_tools}");
    assert!(with_tools.contains("Reply in French and name things by their title"));
    assert!(with_tools
        .ends_with("Always reply in French, even when the user writes in another language."));
    assert_eq!(
        system_prompt(None, &[], None, Some(ReplyLanguage::De), false).as_deref(),
        Some("Always reply in German, even when the user writes in another language.")
    );
    // Both tool instructions carry the sentence the fixed language replaces.
    let without_search = system_prompt(
        None,
        &[tool("run_command")],
        None,
        Some(ReplyLanguage::It),
        false,
    )
    .unwrap();
    assert!(without_search.contains("Reply in Italian and name"));
}

/// Spec 082 research R8: the lookup sentence, in English, in every instruction with tools.
const LOOKUP: &str =
    "Look up what the user names (a person, a subject, a time) with the offered tools; \
                      never ask the user for ids. If several match, ask which one by their titles.";

#[test]
fn every_turn_with_tools_is_told_to_look_up_what_the_user_names() {
    for tools in [vec![tool("find_actions")], vec![tool("wm_apps_list")]] {
        let prompt = system_prompt(None, &tools, None, None, false).unwrap();
        assert!(prompt.contains(LOOKUP), "{prompt}");
    }
    assert_eq!(system_prompt(None, &[], None, None, false), None);
}

/// Spec 087 research R11.2 (FR-018): a search that finds nothing leads to the other offered areas,
/// once, in every instruction of a routed turn and in none of a turn that was not routed.
#[test]
fn a_routed_turn_with_tools_is_told_to_look_in_the_next_area_once() {
    for tools in [vec![tool("find_actions")], vec![tool("wm_apps_list")]] {
        let routed = system_prompt(None, &tools, None, None, true).unwrap();
        assert_eq!(routed.matches(NEXT_AREA).count(), 1, "{routed}");
        // Before the closing sentence, where it was measured.
        assert!(
            routed.contains(&format!(
                "{NEXT_AREA} For questions that have nothing to do"
            )),
            "{routed}"
        );
        let unrouted = system_prompt(None, &tools, None, None, false).unwrap();
        assert!(!unrouted.contains(NEXT_AREA), "{unrouted}");
    }
}

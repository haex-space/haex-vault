use serde_json::json;

use super::prompt::{now_line, system_prompt, TOOL_INSTRUCTION};
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
        system_prompt(None, &[tool("find_actions")], None).as_deref(),
        Some(TOOL_INSTRUCTION)
    );
    assert_eq!(
        system_prompt(Some("  "), &[tool("find_actions")], None).as_deref(),
        Some(TOOL_INSTRUCTION)
    );
}

#[test]
fn a_given_prompt_comes_first() {
    let prompt = system_prompt(Some("Be brief."), &[tool("find_actions")], None).unwrap();
    assert!(prompt.starts_with("Be brief.\n\n"));
    assert!(prompt.ends_with(TOOL_INSTRUCTION));
}

#[test]
fn a_turn_without_tools_is_left_as_it_is() {
    assert_eq!(system_prompt(None, &[], None), None);
    assert_eq!(
        system_prompt(Some("Be brief."), &[], None).as_deref(),
        Some("Be brief.")
    );
}

#[test]
fn a_turn_without_search_does_not_mention_an_unavailable_tool() {
    let prompt = system_prompt(None, &[tool("run_command")], None).unwrap();
    assert!(!prompt.contains("find_actions"));
    assert!(prompt.contains("call one of the offered tools"));
}

#[test]
fn the_instruction_names_the_search_tool_it_relies_on() {
    assert!(TOOL_INSTRUCTION.contains(crate::chat::tools::offer::FIND_ACTIONS_TOOL_NAME));
}

#[test]
fn a_turn_offering_ask_user_is_told_to_ask_instead_of_refusing() {
    let with = system_prompt(None, &[tool("find_actions"), tool("ask_user")], None).unwrap();
    assert!(with.contains("ask_user"));
    assert!(with.contains("instead of refusing"));
    let without = system_prompt(None, &[tool("find_actions")], None).unwrap();
    assert!(!without.contains("ask_user"));
}

#[test]
fn every_turn_with_tools_is_told_to_name_things_by_their_title_not_their_id() {
    for tools in [vec![tool("find_actions")], vec![tool("run_command")]] {
        let prompt = system_prompt(None, &tools, None).unwrap();
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
        "The user's local date and time now: Tuesday, 2026-11-03 14:05 (UTC+01:00)."
    );
}

#[test]
fn an_offset_west_of_utc_and_a_new_day_there_are_told_as_they_are() {
    let clock = Clock::at(1_793_711_100, -(13 * 60 + 30));
    assert_eq!(clock.now, Civil::new(2026, 11, 2, 23, 35));
    assert_eq!(
        now_line(clock),
        "The user's local date and time now: Monday, 2026-11-02 23:35 (UTC-13:30)."
    );
}

#[test]
fn every_turn_with_a_clock_is_told_the_date_also_without_tools() {
    let clock = Some(tuesday_afternoon());
    let line = now_line(tuesday_afternoon());
    assert_eq!(system_prompt(None, &[], clock), Some(line.clone()));
    assert_eq!(
        system_prompt(Some("Be brief."), &[], clock),
        Some(format!("Be brief.\n\n{line}"))
    );
    let with_tools = system_prompt(None, &[tool("find_actions")], clock).unwrap();
    assert_eq!(with_tools, format!("{TOOL_INSTRUCTION}\n\n{line}"));
}

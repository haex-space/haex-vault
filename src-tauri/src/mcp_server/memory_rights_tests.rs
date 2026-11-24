//! The rights group `memory` (spec 097 data-model.md "Rights", research R3, T024): its levels, its
//! row, its JSON form and the scopes it opens.

use serde_json::json;

use super::grant_rows::{names_memory, rights_of, rows_of, GrantRow};
use super::grants::{covers_local, default_rights, MemoryLevel, Rights};
use crate::chat::tools::action_tool::AgentActionDef;

fn memory_action(id: &str, scope: &str, effect: &str) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": id.replace('.', "_"),
        "actionId": id,
        "description": "d",
        "inputSchema": { "type": "object" },
        "effect": effect,
        "scope": scope,
        "runner": "native",
    }))
    .expect("definition")
}

fn with(level: MemoryLevel) -> Rights {
    Rights {
        memory: level,
        ..Rights::default()
    }
}

fn row(level: &str) -> GrantRow {
    GrantRow {
        group: "memory".to_owned(),
        key: String::new(),
        level: level.to_owned(),
        tags: None,
    }
}

#[test]
fn a_new_profile_has_no_memory_and_the_factory_reads_and_writes() {
    assert_eq!(Rights::default().memory, MemoryLevel::Off);
    assert_eq!(default_rights().memory, MemoryLevel::ReadWrite);
}

#[test]
fn every_level_is_one_row_and_reads_back() {
    for (level, stored) in [
        (MemoryLevel::Off, "off"),
        (MemoryLevel::Read, "read"),
        (MemoryLevel::ReadWrite, "read_write"),
    ] {
        let rows = rows_of(&with(level));
        let memory: Vec<&GrantRow> = rows.iter().filter(|r| r.group == "memory").collect();
        assert_eq!(
            memory,
            vec![&row(stored)],
            "{level:?} is written, also at off"
        );
        assert!(names_memory(&rows));
        assert_eq!(rights_of(rows).memory, level);
    }
}

#[test]
fn a_level_this_build_cannot_read_is_off() {
    assert_eq!(rights_of([row("admin")]).memory, MemoryLevel::Off);
    assert_eq!(rights_of([row("")]).memory, MemoryLevel::Off);
    assert!(!names_memory(&[]));
}

#[test]
fn the_json_form_is_that_of_the_contract() {
    assert_eq!(
        serde_json::to_value(with(MemoryLevel::ReadWrite)).expect("json")["memory"],
        json!("readWrite")
    );
    assert_eq!(
        serde_json::to_value(with(MemoryLevel::Read)).expect("json")["memory"],
        json!("read")
    );
    let parsed: Rights = serde_json::from_value(json!({ "operate": {} })).expect("rights");
    assert_eq!(
        parsed.memory,
        MemoryLevel::Off,
        "a window without the field sends off"
    );
}

#[test]
fn the_levels_open_the_two_scopes() {
    let read = memory_action("memory.search", "memory.read", "read");
    let write = memory_action("memory.add", "memory.write", "write");
    for (level, reads, writes) in [
        (MemoryLevel::Off, false, false),
        (MemoryLevel::Read, true, false),
        (MemoryLevel::ReadWrite, true, true),
    ] {
        let rights = with(level);
        assert_eq!(covers_local(&rights, &read), reads, "{level:?} read");
        assert_eq!(covers_local(&rights, &write), writes, "{level:?} write");
    }
}

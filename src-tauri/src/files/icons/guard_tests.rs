//! FR-016 (spec 053, T060): the icon sets are the app's own window's business. No bridge method of
//! an extension reads or writes them, and the Tauri commands stay out of every list an extension or
//! an agent is given.

/// The bridge methods an extension frame can call, as the source of their allowlist names them.
const ALLOWLIST: &str = include_str!("../../extensions/bridge/allowlist.rs");
/// The lists of commands that cross into the extension host and the agent tools.
const SOURCES: [(&str, &str); 3] = [
    ("bridge allowlist", ALLOWLIST),
    (
        "bridge methods",
        include_str!("../../extensions/bridge/methods.rs"),
    ),
    (
        "extension agent tools",
        include_str!("../../extensions/agent_tools/mod.rs"),
    ),
];

const FORBIDDEN: [&str; 6] = [
    "files::icons",
    "files_icon",
    "file_icon_set",
    "iconSet",
    "files.iconSet",
    "haex_file_icon_sets",
];

#[test]
fn no_bridge_method_and_no_agent_tool_reaches_the_icon_sets() {
    for (place, source) in SOURCES {
        for word in FORBIDDEN {
            assert!(
                !source.contains(word),
                "{place} names {word}: the icon sets are not for extensions or agents"
            );
        }
    }
}

#[test]
fn the_choice_has_a_key_of_its_own_that_the_settings_actions_do_not_name() {
    // The one key the window writes; the action catalog (`settingsActions.ts`) never names it,
    // which `scripts/check-file-icons.ts` checks from the other side.
    assert_eq!(super::SELECTION_KEY, "files.iconSet");
}

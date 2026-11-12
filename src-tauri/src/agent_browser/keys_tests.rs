//! Spec 065 T111: which keys an agent may press (FR-049).

use super::*;

fn press(key: &str, modifiers: &[Modifier]) -> KeyPress {
    resolve(key, modifiers).unwrap_or_else(|error| panic!("{key} {modifiers:?}: {error:?}"))
}

#[test]
fn every_named_key_maps_to_its_codes() {
    let cases = [
        ("Enter", "Enter", 13, Some("\r")),
        ("Escape", "Escape", 27, None),
        ("Tab", "Tab", 9, None),
        ("Backspace", "Backspace", 8, None),
        ("Delete", "Delete", 46, None),
        ("Space", "Space", 32, Some(" ")),
        ("ArrowUp", "ArrowUp", 38, None),
        ("ArrowDown", "ArrowDown", 40, None),
        ("ArrowLeft", "ArrowLeft", 37, None),
        ("ArrowRight", "ArrowRight", 39, None),
        ("Home", "Home", 36, None),
        ("End", "End", 35, None),
        ("PageUp", "PageUp", 33, None),
        ("PageDown", "PageDown", 34, None),
    ];
    for (name, code, virtual_key, text) in cases {
        let key = press(name, &[]);
        assert_eq!(key.code, code, "{name}");
        assert_eq!(key.virtual_key, virtual_key, "{name}");
        assert_eq!(key.text.as_deref(), text, "{name}");
        assert!(key.commands.is_empty());
        assert_eq!(key.modifiers, 0);
    }
}

#[test]
fn a_space_character_is_the_space_key() {
    assert_eq!(press(" ", &[]), press("Space", &[]));
}

#[test]
fn one_printable_character_types_itself() {
    let a = press("a", &[]);
    assert_eq!(
        (a.key.as_str(), a.code.as_str(), a.virtual_key),
        ("a", "KeyA", 65)
    );
    assert_eq!(a.text.as_deref(), Some("a"));
    let seven = press("7", &[]);
    assert_eq!((seven.code.as_str(), seven.virtual_key), ("Digit7", 55));
    let slash = press("/", &[]);
    assert_eq!(slash.text.as_deref(), Some("/"));
    let umlaut = press("ä", &[]);
    assert_eq!(umlaut.text.as_deref(), Some("ä"));
}

#[test]
fn shift_goes_with_every_key() {
    assert_eq!(press("a", &[Modifier::Shift]).text.as_deref(), Some("A"));
    assert_eq!(press("Tab", &[Modifier::Shift]).modifiers, 8);
    assert_eq!(press("ArrowLeft", &[Modifier::Shift]).modifiers, 8);
    assert_eq!(press("Home", &[Modifier::Shift]).modifiers, 8);
}

#[test]
fn ctrl_and_meta_go_only_with_a_z_y() {
    for (key, command) in [
        ("a", "selectAll"),
        ("z", "undo"),
        ("y", "redo"),
        ("A", "selectAll"),
    ] {
        for modifier in [Modifier::Ctrl, Modifier::Meta] {
            let press = press(key, &[modifier]);
            assert_eq!(press.commands, vec![command], "{key}");
            // A shortcut types nothing.
            assert_eq!(press.text, None);
        }
    }
    assert_eq!(press("a", &[Modifier::Ctrl]).modifiers, 2);
    assert_eq!(press("a", &[Modifier::Meta]).modifiers, 4);
    assert_eq!(press("z", &[Modifier::Ctrl, Modifier::Shift]).modifiers, 10);
}

#[test]
fn clipboard_and_browser_shortcuts_are_refused() {
    for key in ["c", "x", "v", "w", "t", "n", "l", "r", "p", "s", "f", "1"] {
        for modifier in [Modifier::Ctrl, Modifier::Meta] {
            assert!(
                matches!(resolve(key, &[modifier]), Err(KeyError::NotAllowed(_))),
                "{modifier:?}+{key}"
            );
        }
    }
}

#[test]
fn a_chord_with_a_named_key_is_refused() {
    for name in [
        "Enter",
        "Backspace",
        "Delete",
        "ArrowLeft",
        "Tab",
        "Space",
        "End",
    ] {
        assert!(matches!(
            resolve(name, &[Modifier::Ctrl]),
            Err(KeyError::NotAllowed(_))
        ));
        assert!(matches!(
            resolve(name, &[Modifier::Meta]),
            Err(KeyError::NotAllowed(_))
        ));
    }
}

#[test]
fn alt_is_never_allowed() {
    for key in ["ArrowLeft", "a", "Enter", "Tab", "F4"] {
        assert!(matches!(
            resolve(key, &[Modifier::Alt]),
            Err(KeyError::NotAllowed(_))
        ));
    }
    assert!(resolve("a", &[Modifier::Shift, Modifier::Alt]).is_err());
}

#[test]
fn function_keys_and_unknown_names_are_refused() {
    for key in [
        "F1",
        "F5",
        "F11",
        "F12",
        "Insert",
        "ContextMenu",
        "Meta",
        "Control",
        "Shift",
        "Alt",
        "CapsLock",
        "enter",
        "",
        "ab",
        "\n",
        "\t",
        "\u{7f}",
    ] {
        assert!(
            matches!(resolve(key, &[]), Err(KeyError::NotAllowed(_))),
            "{key:?}"
        );
    }
}

#[test]
fn count_is_one_to_twenty() {
    assert_eq!(count(None), Ok(1));
    assert_eq!(count(Some(1)), Ok(1));
    assert_eq!(count(Some(20)), Ok(20));
    assert_eq!(count(Some(0)), Err(KeyError::Count));
    assert_eq!(count(Some(21)), Err(KeyError::Count));
}

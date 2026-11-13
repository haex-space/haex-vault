//! The keys an agent may press (spec 065 FR-049, research R19). This is the one place that decides
//! it: a key not on the list never reaches the page. The list is an allowlist because a
//! synthesized `Ctrl+T` or `Ctrl+N` reaches the browser's own handling and opens a tab or a window
//! (spike S1); the keys that act on the browser are the ones a blocklist would forget.
//!
//! Allowed: Enter, Escape, Tab, Backspace, Delete, Space, the arrows, Home, End, PageUp, PageDown
//! and one printable character. `Shift` goes with all of them; `Ctrl` and `Meta` only with A, Z
//! and Y (select all, undo, redo), because `Ctrl+C`, `X` and `V` reach the user's clipboard.
//! `Alt`, function keys and every other combination are refused.

use serde::Deserialize;

/// The most key presses one call repeats.
pub const MAX_COUNT: u32 = 20;

/// A modifier an agent may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    Shift,
    Ctrl,
    Meta,
    /// Never allowed; named so the refusal says why instead of failing to parse.
    Alt,
}

impl Modifier {
    /// The bit of `Input.dispatchKeyEvent`'s `modifiers`.
    fn bit(self) -> u8 {
        match self {
            Self::Alt => 1,
            Self::Ctrl => 2,
            Self::Meta => 4,
            Self::Shift => 8,
        }
    }
}

/// Why a key is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// The key or the combination is not on the list (`key_not_allowed`).
    NotAllowed(String),
    /// `count` is outside 1 to [`MAX_COUNT`].
    Count,
}

/// One key press, as `Input.dispatchKeyEvent` takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPress {
    pub key: String,
    pub code: String,
    pub virtual_key: u32,
    /// The character the key types; `None` for keys that type nothing (arrows, Escape, ...).
    pub text: Option<String>,
    /// Editing commands Chromium runs for a synthesized shortcut (`selectAll`, `undo`, `redo`).
    pub commands: Vec<&'static str>,
    pub modifiers: u8,
}

/// Names that need a table: `(name, key, code, virtual key, text)`.
const NAMED: &[(&str, &str, &str, u32, Option<&str>)] = &[
    ("Enter", "Enter", "Enter", 13, Some("\r")),
    ("Escape", "Escape", "Escape", 27, None),
    ("Tab", "Tab", "Tab", 9, None),
    ("Backspace", "Backspace", "Backspace", 8, None),
    ("Delete", "Delete", "Delete", 46, None),
    ("Space", " ", "Space", 32, Some(" ")),
    ("ArrowUp", "ArrowUp", "ArrowUp", 38, None),
    ("ArrowDown", "ArrowDown", "ArrowDown", 40, None),
    ("ArrowLeft", "ArrowLeft", "ArrowLeft", 37, None),
    ("ArrowRight", "ArrowRight", "ArrowRight", 39, None),
    ("Home", "Home", "Home", 36, None),
    ("End", "End", "End", 35, None),
    ("PageUp", "PageUp", "PageUp", 33, None),
    ("PageDown", "PageDown", "PageDown", 34, None),
];

fn refused(key: &str) -> KeyError {
    KeyError::NotAllowed(key.to_owned())
}

/// The press of `key` with `modifiers`, or why it is refused.
pub fn resolve(key: &str, modifiers: &[Modifier]) -> Result<KeyPress, KeyError> {
    if modifiers.contains(&Modifier::Alt) {
        return Err(refused(key));
    }
    let bits = modifiers.iter().fold(0, |bits, m| bits | m.bit());
    let chorded = modifiers
        .iter()
        .any(|m| matches!(m, Modifier::Ctrl | Modifier::Meta));

    // " " names the space key as well as "Space".
    let name = if key == " " { "Space" } else { key };
    if let Some(&(_, key, code, virtual_key, text)) = NAMED.iter().find(|row| row.0 == name) {
        // A chord with a named key (Ctrl+Enter, Meta+Backspace) is not on the list.
        if chorded {
            return Err(refused(name));
        }
        return Ok(KeyPress {
            key: key.to_owned(),
            code: code.to_owned(),
            virtual_key,
            text: text.map(str::to_owned),
            commands: Vec::new(),
            modifiers: bits,
        });
    }

    let mut chars = key.chars();
    let (Some(character), None) = (chars.next(), chars.next()) else {
        return Err(refused(key));
    };
    if character.is_control() || character.is_whitespace() {
        return Err(refused(key));
    }
    if chorded {
        let shifted = modifiers.contains(&Modifier::Shift);
        let command = match character.to_ascii_lowercase() {
            'a' => "selectAll",
            // Ctrl+Shift+Z is the redo shortcut of most platforms.
            'z' if shifted => "redo",
            'z' => "undo",
            'y' => "redo",
            _ => return Err(refused(key)),
        };
        // A shortcut types nothing; the command does the work.
        return Ok(KeyPress {
            key: character.to_ascii_lowercase().to_string(),
            code: format!("Key{}", character.to_ascii_uppercase()),
            virtual_key: u32::from(character.to_ascii_uppercase()),
            text: None,
            commands: vec![command],
            modifiers: bits,
        });
    }

    let shifted = modifiers.contains(&Modifier::Shift);
    let typed = if shifted && character.is_ascii_alphabetic() {
        character.to_ascii_uppercase()
    } else {
        character
    };
    let (code, virtual_key) = if character.is_ascii_alphabetic() {
        (
            format!("Key{}", character.to_ascii_uppercase()),
            u32::from(character.to_ascii_uppercase()),
        )
    } else if character.is_ascii_digit() {
        (format!("Digit{character}"), u32::from(character))
    } else {
        (String::new(), 0)
    };
    Ok(KeyPress {
        key: typed.to_string(),
        code,
        virtual_key,
        text: Some(typed.to_string()),
        commands: Vec::new(),
        modifiers: bits,
    })
}

/// `count` of a call, 1 when it is missing.
pub fn count(count: Option<u32>) -> Result<u32, KeyError> {
    match count.unwrap_or(1) {
        n @ 1..=MAX_COUNT => Ok(n),
        _ => Err(KeyError::Count),
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;

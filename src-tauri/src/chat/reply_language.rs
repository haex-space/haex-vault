//! The language the agent answers in (operator 2026-11-03, spec 066 FR-002a): by default the
//! interface language. Spoken conversations can take another one, chosen per device on the
//! assistant's page, for example to practise a language; they then also listen and speak in it.

use uuid::Uuid;

use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

/// The device preference of the assistant: `"ui"` (or unset) follows the interface, a language
/// code overrides it for spoken conversations.
pub const PREF_CONVERSATION_LANGUAGE: &str = "assistant.language";
/// The interface language of the vault (spec 042).
pub const PREF_UI_LANGUAGE: &str = "general.language";

/// A language the agent can be told to answer in: the languages the voice assistant speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyLanguage {
    De,
    En,
    Fr,
    Es,
    It,
    Pt,
}

impl ReplyLanguage {
    /// Reads a stored code; anything else is `None`.
    pub fn from_code(code: &str) -> Option<Self> {
        Some(match code {
            "de" => Self::De,
            "en" => Self::En,
            "fr" => Self::Fr,
            "es" => Self::Es,
            "it" => Self::It,
            "pt" => Self::Pt,
            _ => return None,
        })
    }

    /// The code the voice models and the word lists use.
    pub fn code(self) -> &'static str {
        match self {
            Self::De => "de",
            Self::En => "en",
            Self::Fr => "fr",
            Self::Es => "es",
            Self::It => "it",
            Self::Pt => "pt",
        }
    }

    /// The name in the system prompt, which is English (prompt.rs).
    pub fn english_name(self) -> &'static str {
        match self {
            Self::De => "German",
            Self::En => "English",
            Self::Fr => "French",
            Self::Es => "Spanish",
            Self::It => "Italian",
            Self::Pt => "Portuguese",
        }
    }
}

/// The language of a turn: for a spoken one the language chosen for conversations, else the
/// interface language; `None` when neither is known (the model then follows the user).
pub fn resolve(conversation: Option<&str>, ui: Option<&str>) -> Option<ReplyLanguage> {
    conversation
        .and_then(ReplyLanguage::from_code)
        .or_else(|| ui.and_then(ReplyLanguage::from_code))
}

/// The language of a typed turn: the vault's interface language.
pub fn read_typed(q: &mut impl Query) -> haex_crdt::Result<Option<ReplyLanguage>> {
    let ui = preferences::get(q, PrefScope::Vault, PREF_UI_LANGUAGE)?;
    Ok(resolve(None, ui.as_deref()))
}

/// The language of a spoken conversation on `device`: its choice on the assistant page, else the
/// interface language.
pub fn read_spoken(q: &mut impl Query, device: Uuid) -> haex_crdt::Result<Option<ReplyLanguage>> {
    let conversation = preferences::get(q, PrefScope::Device(device), PREF_CONVERSATION_LANGUAGE)?;
    if let Some(language) = conversation.as_deref().and_then(ReplyLanguage::from_code) {
        return Ok(Some(language));
    }
    let ui = preferences::get(q, PrefScope::Vault, PREF_UI_LANGUAGE)?;
    Ok(resolve(None, ui.as_deref()))
}

#[cfg(test)]
#[path = "reply_language_tests.rs"]
mod tests;

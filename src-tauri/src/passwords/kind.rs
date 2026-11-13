//! The type of an entry (spec 092, data-model.md, research R1): a `login` or a `pass` (a card or a
//! ticket). The stored text is `haex_passwords_item_details.kind`. An entry written by a newer
//! holzi can carry a type this build does not know; it is carried as text, never defaulted to a
//! login, so the window can say "needs a newer holzi" and nothing writes it over (FR-042).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// What an entry is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EntryType {
    #[default]
    Login,
    Pass,
    /// A type of a newer build; the stored text, kept as received.
    Unknown(String),
}

impl EntryType {
    pub const LOGIN: &'static str = "login";
    pub const PASS: &'static str = "pass";

    /// The type of a stored text; an empty or missing text is a login (a vault from before the
    /// column holds none).
    pub fn parse(text: &str) -> Self {
        match text {
            "" | Self::LOGIN => Self::Login,
            Self::PASS => Self::Pass,
            other => Self::Unknown(other.to_owned()),
        }
    }

    pub fn as_text(&self) -> &str {
        match self {
            Self::Login => Self::LOGIN,
            Self::Pass => Self::PASS,
            Self::Unknown(text) => text,
        }
    }

    pub fn is_login(&self) -> bool {
        matches!(self, Self::Login)
    }

    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }
}

impl fmt::Display for EntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_text())
    }
}

impl Serialize for EntryType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_text())
    }
}

impl<'de> Deserialize<'de> for EntryType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

/// Which types a reader asks for. Every reader that serves a credential, an autofill or a secret
/// asks for [`Kinds::Logins`], and that is the default everywhere outside the user's own window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kinds {
    #[default]
    Logins,
    Passes,
    Any,
}

impl Kinds {
    pub fn admits(self, kind: &EntryType) -> bool {
        match self {
            Self::Logins => kind.is_login(),
            Self::Passes => kind.is_pass(),
            Self::Any => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_type_is_carried_not_defaulted() {
        assert_eq!(EntryType::parse("login"), EntryType::Login);
        assert_eq!(EntryType::parse(""), EntryType::Login);
        assert_eq!(EntryType::parse("pass"), EntryType::Pass);
        let newer = EntryType::parse("payment_card");
        assert_eq!(newer, EntryType::Unknown("payment_card".into()));
        assert_eq!(newer.as_text(), "payment_card");
        assert!(!newer.is_login() && !newer.is_pass());
        assert_eq!(serde_json::to_string(&newer).unwrap(), "\"payment_card\"");
        assert_eq!(
            serde_json::from_str::<EntryType>("\"pass\"").unwrap(),
            EntryType::Pass
        );
    }

    #[test]
    fn the_filters_admit_what_they_name() {
        let unknown = EntryType::Unknown("x".into());
        assert!(Kinds::Logins.admits(&EntryType::Login));
        assert!(!Kinds::Logins.admits(&EntryType::Pass));
        assert!(!Kinds::Logins.admits(&unknown));
        assert!(Kinds::Passes.admits(&EntryType::Pass));
        assert!(!Kinds::Passes.admits(&EntryType::Login));
        assert!(Kinds::Any.admits(&unknown));
    }
}

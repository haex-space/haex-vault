//! The shared memory of the agents (spec 097): one set of Markdown pages per vault, kept in the
//! CRDT-tracked table `haex_agent_memory_pages` and therefore the same on every own device
//! (FR-001, FR-004).
//!
//! A page has a title, a one-line description and a text; pages point to each other by `[[Title]]`
//! in the text (FR-002, FR-029). The pointers are read from the text and never stored
//! ([`links`]). The user edits the pages in "KI → Gedächtnis" ([`commands`]); agents read the table
//! of contents at the start of a conversation and open, search and write pages through their
//! actions (spec 097 PR B and C). Every write is held to the limits ([`limits`]) and the check
//! ([`check`]) whoever writes (FR-003, FR-022).

pub mod check;
pub mod commands;
pub mod executor;
pub mod fill;
pub mod foreign;
pub mod gate;
pub mod limits;
pub mod links;
pub mod search;
pub mod store;
pub mod toc;
pub mod turn;

use uuid::Uuid;

/// Who wrote or changed a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorKind {
    /// The user, in the window.
    User,
    /// A model of the local agent or of an Abo-Agent.
    Model,
}

impl AuthorKind {
    /// The value stored in `author_kind` and `changed_kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Model => "model",
        }
    }

    /// The kind of a stored value; `None` for a value this build does not know (data-model.md
    /// "Unknown values": shown by its stored name, without a kind).
    pub fn parse(stored: &str) -> Option<Self> {
        match stored {
            "user" => Some(Self::User),
            "model" => Some(Self::Model),
            _ => None,
        }
    }
}

/// The origin of a write (FR-002): the writer and the vault device it wrote on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub kind: AuthorKind,
    /// The subject key of spec 083 (`model:<id>`); `None` for the user.
    pub key: Option<String>,
    /// The name the user knew the model by when it wrote; empty for the user. Stored, not looked
    /// up, so a page keeps the name of a model that was deleted.
    pub name: String,
    /// The vault device UUID of the device that wrote.
    pub device: Uuid,
}

impl Author {
    /// The user, writing on `device`.
    pub fn user(device: Uuid) -> Self {
        Self {
            kind: AuthorKind::User,
            key: None,
            name: String::new(),
            device,
        }
    }
}

/// How titles are compared: without case and surrounding spaces (data-model.md "Title").
pub fn title_key(title: &str) -> String {
    title.trim().to_lowercase()
}

//! The shapes of the SDK's mail types (`vault-sdk` `src/types/mail.ts`).

use std::fmt;

use serde::{Deserialize, Serialize};

/// How the connection to the server is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionSecurity {
    /// TLS from the first byte (IMAPS 993, SMTPS 465).
    Tls,
    /// Plain connect, then STARTTLS (IMAP 143, submission 587).
    StartTls,
    /// No encryption: only to this device (`super::check_server`).
    None,
}

/// A server and the credentials for it, as the SDK's `ImapConfig` and `SmtpConfig`.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub security: ConnectionSecurity,
    pub username: String,
    pub password: String,
}

impl fmt::Debug for ServerConfig {
    /// Never prints the password.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("security", &self.security)
            .field("password", &"<redacted>")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    pub flags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exists: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unseen: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid_validity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid_next: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEnvelope {
    pub uid: u32,
    pub flags: Vec<String>,
    /// The server's internal date, Unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_date: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub from: Vec<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub part_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub content_type: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
    pub is_inline: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub envelope: MessageEnvelope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_html: Option<String>,
    pub attachments: Vec<Attachment>,
}

/// Which messages a fetch covers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum FetchRange {
    /// The last `count` messages of the mailbox.
    Latest {
        count: u32,
    },
    UidRange {
        start: u32,
        end: u32,
    },
    UidList {
        uids: Vec<u32>,
    },
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingAttachment {
    pub filename: String,
    pub content_type: String,
    /// Base64, standard alphabet.
    pub data: String,
    #[serde(default)]
    pub content_id: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingMessage {
    pub from: Address,
    pub to: Vec<Address>,
    #[serde(default)]
    pub cc: Vec<Address>,
    #[serde(default)]
    pub bcc: Vec<Address>,
    #[serde(default)]
    pub reply_to: Option<Address>,
    pub subject: String,
    #[serde(default)]
    pub body_text: Option<String>,
    #[serde(default)]
    pub body_html: Option<String>,
    #[serde(default)]
    pub attachments: Vec<OutgoingAttachment>,
    #[serde(default)]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
}

/// The criteria of `extension_mail_search`, all of which must hold (AND). Not an IMAP expression:
/// the host builds the command itself, so text from a message or a model never becomes a command.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchCriteria {
    /// Header and body text.
    pub text: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub subject: Option<String>,
    /// `YYYY-MM-DD`, inclusive.
    pub since: Option<String>,
    /// `YYYY-MM-DD`, exclusive.
    pub before: Option<String>,
    /// `true` for unread, `false` for read.
    pub unseen: Option<bool>,
    /// `true` for flagged, `false` for not flagged.
    pub flagged: Option<bool>,
}

/// The newest hits of a search, newest first, and how many there are in all.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub uids: Vec<u32>,
    pub total: u32,
}

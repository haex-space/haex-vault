//! Data structures for IMAP/SMTP operations.
//!
//! All public types are TS-exported via `ts-rs` so the same shapes are
//! available to extensions through the SDK without manual duplication.

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How the transport secures the connection to the mail server.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConnectionSecurity {
    /// Implicit TLS from connect (IMAPS port 993, SMTPS port 465).
    Tls,
    /// Plain connect, then upgrade via STARTTLS (IMAP port 143, SMTP port 587).
    StartTls,
    /// No encryption. ONLY for testing against local servers — never use in production.
    None,
}

/// IMAP server configuration + credentials.
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub security: ConnectionSecurity,
    pub username: String,
    pub password: String,
}

impl fmt::Debug for ImapConfig {
    // Manual impl so accidental tracing/dbg!/println! never leaks the password.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("security", &self.security)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// SMTP server configuration + credentials.
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub security: ConnectionSecurity,
    pub username: String,
    pub password: String,
}

impl fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("security", &self.security)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// A mail account: IMAP for fetch, SMTP for send.
///
/// SMTP is optional so an account can be fetch-only.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Account {
    /// Caller-defined identifier. Used by future connection-pool keying;
    /// must be stable per logical account.
    pub id: String,
    pub imap: ImapConfig,
    pub smtp: Option<SmtpConfig>,
}

/// Email address, optionally with display name.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Address {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub email: String,
}

/// Mailbox / folder metadata returned by LIST + STATUS.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailboxInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    /// Mailbox flags reported by LIST (e.g. "\\HasNoChildren", "\\Marked").
    pub flags: Vec<String>,
    /// Total message count (only set if STATUS was queried).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exists: Option<u32>,
    /// Unseen / unread count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unseen: Option<u32>,
    /// UIDVALIDITY — if it changes the local UID-cache must be invalidated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid_validity: Option<u32>,
    /// UIDNEXT — predicted UID of the next arriving message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid_next: Option<u32>,
}

/// Lightweight message summary for list views (no body).
///
/// `references` and `in_reply_to` enable client-side threading.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MessageEnvelope {
    pub uid: u32,
    /// IMAP per-flags (e.g. "\\Seen", "\\Answered", "\\Flagged").
    pub flags: Vec<String>,
    /// Server-side internal date as Unix timestamp (seconds).
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
}

/// Full message: envelope + parsed body parts + attachment metadata.
///
/// Attachment data is NOT included by default — request individual
/// attachments via a separate fetch (TODO: dedicated attachment API).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Message {
    pub envelope: MessageEnvelope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_html: Option<String>,
    pub attachments: Vec<Attachment>,
}

/// Attachment metadata returned in `Message.attachments`. Body data is
/// fetched separately to keep list/read responses lean.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Attachment {
    /// Index of this part in the parsed message (callers use it to
    /// request the data on demand).
    pub part_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub content_type: String,
    pub size: u64,
    /// Content-ID for inline images referenced by HTML body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
    pub is_inline: bool,
}

/// Selector for `fetch_envelopes` / `fetch_message`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "type")]
#[ts(export)]
pub enum FetchRange {
    /// Last N messages by sequence number (descending). The IMAP
    /// adapter translates this to a sequence-set against EXISTS.
    Latest { count: u32 },
    /// UID range (inclusive).
    UidRange { start: u32, end: u32 },
    /// Explicit list of UIDs.
    UidList { uids: Vec<u32> },
}

/// Outgoing message for SMTP send.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutgoingMessage {
    pub from: Address,
    pub to: Vec<Address>,
    #[serde(default)]
    pub cc: Vec<Address>,
    #[serde(default)]
    pub bcc: Vec<Address>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Address>,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_html: Option<String>,
    #[serde(default)]
    pub attachments: Vec<OutgoingAttachment>,
    /// Message-ID being replied to. Sets `In-Reply-To` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    /// Threading chain. Sets `References` header.
    #[serde(default)]
    pub references: Vec<String>,
}

/// Outgoing attachment. `data` is base64-encoded — matches the web
/// module's wire format for binary payloads. The SMTP layer decodes
/// before building the MIME part.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutgoingAttachment {
    pub filename: String,
    pub content_type: String,
    /// Base64-encoded bytes (standard alphabet, with padding).
    pub data: String,
    /// If `Some`, sets `Content-ID` and the part is marked inline
    /// (referenced by `cid:` URLs in `body_html`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
}

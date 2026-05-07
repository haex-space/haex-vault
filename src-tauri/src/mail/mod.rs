//! Core mail module.
//!
//! General-purpose IMAP fetch + SMTP send. Lives outside `extension/`
//! because the functionality is not extension-specific — extensions
//! consume it through the wrapper in `extension/mail/` (which adds
//! permission checks + extension-id resolution).
//!
//! Credentials are passed in per call by the caller (typically loaded
//! from the core passwords vault). This module never persists secrets.
//!
//! # Connection model
//!
//! For simplicity Phase 1 connects fresh on every call (TLS handshake
//! + LOGIN + SELECT each time). A connection pool keyed by
//! `Account.id` is a planned optimization once latency profiling shows
//! the reconnect cost is the bottleneck.

pub mod error;
pub mod imap;
pub mod parsing;
pub mod smtp;
pub mod types;

pub use error::MailError;
pub use types::{
    Account, Address, Attachment, ConnectionSecurity, FetchRange, ImapConfig, MailboxInfo,
    Message, MessageEnvelope, OutgoingAttachment, OutgoingMessage, SmtpConfig,
};

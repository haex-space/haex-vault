//! Errors for the core mail module.
//!
//! `MailError` is intentionally separate from `ExtensionError` because
//! the mail module is general-purpose. The extension wrapper in
//! `extension/mail/` converts these into `ExtensionError::WebError` /
//! a future dedicated `ExtensionError::Mail` variant.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum MailError {
    #[error("TCP connection to {host}:{port} failed: {source}")]
    Connect {
        host: String,
        port: u16,
        source: std::io::Error,
    },

    #[error("TLS handshake with {host} failed: {reason}")]
    Tls { host: String, reason: String },

    #[error("IMAP protocol error: {reason}")]
    Imap { reason: String },

    #[error("IMAP authentication failed for user '{username}': {reason}")]
    ImapAuth { username: String, reason: String },

    #[error("SMTP error: {reason}")]
    Smtp { reason: String },

    #[error("SMTP authentication failed for user '{username}': {reason}")]
    SmtpAuth { username: String, reason: String },

    #[error("Message build error: {reason}")]
    MessageBuild { reason: String },

    #[error("Message parse error: {reason}")]
    MessageParse { reason: String },

    #[error("Invalid configuration: {reason}")]
    InvalidConfig { reason: String },
}

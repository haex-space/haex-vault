//! Mail of extensions (spec 017, US11, T108, FR-056, FR-058, research R21): the SDK's
//! `client.mail` on IMAP (`async-imap`) and SMTP (`lettre`), TLS through rustls with ring.
//!
//! Ported from haex-space/haex-vault-legacy `8dce379d94e18fcd42c3b73686a06f984ca3f574`
//! (`src-tauri/src/mail/`, `src-tauri/src/extension/mail/`) and closed where it had gaps:
//! STARTTLS for IMAP (haex-vault 2.x (Legacy) spoke implicit TLS only), no unencrypted connection except to
//! this device, flags and keywords checked as IMAP atoms (haex-vault 2.x put them into the command
//! unchecked), a permission per mail server **and port** (FR-056), answers within the
//! extension's response limit, and no credential in any answer, error or log line.
//!
//! The credentials come with every call from the extension (the SDK's `ImapConfig` and
//! `SmtpConfig`); holzi keeps them only for the length of the call and stores none.

pub mod commands;
mod connect;
mod imap;
mod parsing;
mod smtp;
mod types;
pub mod watch;

use std::net::IpAddr;

use serde_json::json;
use uuid::Uuid;

pub use types::*;

use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::{
    evaluate, Action, Decision, PermissionKind, PermissionRequest, RequestTarget,
};
use crate::vault_gate::VaultDb;

/// What went wrong talking to a mail server. The texts name no credential and no address of
/// a message.
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("could not connect to the server")]
    Connect,
    #[error("TLS with the server failed")]
    Tls,
    #[error("the server refused the login")]
    Auth,
    #[error("IMAP: {0}")]
    Imap(String),
    #[error("SMTP: {0}")]
    Smtp(String),
    #[error("{0}")]
    Invalid(String),
    #[error("the message could not be built: {0}")]
    Build(String),
    #[error("the message could not be read: {0}")]
    Parse(String),
    #[error("not found")]
    NotFound,
    #[error("the answer is too large")]
    TooLarge,
    #[error("the server did not answer in time")]
    Timeout,
}

impl From<MailError> for BridgeError {
    fn from(error: MailError) -> Self {
        let code = match &error {
            MailError::Invalid(_) | MailError::Build(_) => ExtensionErrorCode::Validation,
            MailError::NotFound => ExtensionErrorCode::NotFound,
            MailError::TooLarge | MailError::Timeout => ExtensionErrorCode::LimitExceeded,
            _ => ExtensionErrorCode::Web,
        };
        BridgeError::new(code, error.to_string())
    }
}

/// Whether `host` names this device: only there may a connection go unencrypted (a local bridge
/// or a test server); everything else needs TLS or STARTTLS.
pub fn is_loopback(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// The server a call talks to, checked before anything connects: a host, a port, and no
/// unencrypted connection to another device.
pub fn check_server(host: &str, port: u16, security: ConnectionSecurity) -> Result<(), MailError> {
    if host.is_empty() || host.len() > 253 || host.contains(|c: char| c.is_whitespace() || c == '/')
    {
        return Err(MailError::Invalid("host not allowed".into()));
    }
    if port == 0 {
        return Err(MailError::Invalid("port not allowed".into()));
    }
    if security == ConnectionSecurity::None && !is_loopback(host) {
        return Err(MailError::Invalid(
            "an unencrypted connection is only allowed to this device".into(),
        ));
    }
    Ok(())
}

/// An IMAP flag or keyword: `\Seen` and the other system flags, or an atom (RFC 3501 §9). Nothing
/// that could end the command or start another one.
pub fn check_flag(flag: &str) -> Result<(), MailError> {
    let atom = flag.strip_prefix('\\').unwrap_or(flag);
    let fine = !atom.is_empty()
        && atom.len() <= 64
        && atom.bytes().all(|b| {
            b.is_ascii_graphic()
                && !matches!(b, b'(' | b')' | b'{' | b'%' | b'*' | b'"' | b'\\' | b']')
        });
    if fine {
        Ok(())
    } else {
        Err(MailError::Invalid("flag not allowed".into()))
    }
}

/// A mailbox name: any text without control characters (async-imap quotes it).
pub fn check_mailbox(name: &str) -> Result<(), MailError> {
    if name.is_empty() || name.len() > 1024 || name.chars().any(char::is_control) {
        return Err(MailError::Invalid("mailbox name not allowed".into()));
    }
    Ok(())
}

/// What the permissions of `extension_id` on `device` say to `action` on `server:port`, the held
/// decisions of this process included.
pub fn decision(
    db: &VaultDb,
    host: &ExtensionHost,
    extension_id: Uuid,
    device: Uuid,
    action: &Action,
    server: &str,
    port: u16,
) -> Result<Decision, BridgeError> {
    let mut grants = db
        .read_blocking(move |q| {
            candidates(q, extension_id, PermissionKind::Mail, device).map_err(Into::into)
        })
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))?;
    grants.extend(
        host.permissions
            .temporary(extension_id, PermissionKind::Mail),
    );
    let request = PermissionRequest {
        kind: PermissionKind::Mail,
        action: action.clone(),
        target: RequestTarget::MailServer {
            host: server.to_ascii_lowercase(),
            port,
        },
    };
    Ok(evaluate(&grants, &request, device))
}

/// Allowed, or the 1002/1004 answer for `action` on `host:port` (FR-056).
pub fn check_permission(
    ctx: &CallContext,
    action: Action,
    host: &str,
    port: u16,
) -> Result<(), BridgeError> {
    let decided = decision(
        &ctx.db,
        &ctx.host,
        ctx.session.extension_id,
        ctx.device,
        &action,
        host,
        port,
    )?;
    let code = match decided {
        Decision::Allow => return Ok(()),
        Decision::Deny => ExtensionErrorCode::PermissionDenied,
        Decision::Prompt => ExtensionErrorCode::PermissionPromptRequired,
    };
    let host = host.to_ascii_lowercase();
    Err(
        BridgeError::new(code, "permission required").with_details(json!({
            "resourceType": "mail",
            "action": action.as_string(),
            "target": format!("{host}:{port}"),
        })),
    )
}

#[cfg(test)]
mod mail_tests;
#[cfg(test)]
mod test_server;
#[cfg(test)]
mod test_setup;
#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
#[cfg(test)]
mod watch_tests;

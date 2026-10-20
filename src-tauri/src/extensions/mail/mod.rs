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
mod imap_manage;
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

/// The longest search text, mailbox-independent: a hit on a longer text is a hit on its start.
pub const MAX_SEARCH_TEXT: usize = 200;

/// `YYYY-MM-DD` as year, month, day, when it names a real day.
pub fn parse_date(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return None;
    }
    let (year, month, day): (u32, u32, u32) =
        (year.parse().ok()?, month.parse().ok()?, day.parse().ok()?);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let longest = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    (year >= 1900 && (1..=longest).contains(&day)).then_some((year, month, day))
}

/// A search that is allowed to reach the server: at least one criterion, texts without control
/// characters and at most [`MAX_SEARCH_TEXT`] long, dates that exist.
pub fn check_criteria(criteria: &SearchCriteria) -> Result<(), MailError> {
    let texts = [
        &criteria.text,
        &criteria.from,
        &criteria.to,
        &criteria.subject,
    ];
    for text in texts.into_iter().flatten() {
        if text.trim().is_empty()
            || text.chars().count() > MAX_SEARCH_TEXT
            || text.chars().any(char::is_control)
        {
            return Err(MailError::Invalid("search text not allowed".into()));
        }
    }
    for date in [&criteria.since, &criteria.before].into_iter().flatten() {
        if parse_date(date).is_none() {
            return Err(MailError::Invalid("date must be YYYY-MM-DD".into()));
        }
    }
    let any = texts.iter().any(|t| t.is_some())
        || criteria.since.is_some()
        || criteria.before.is_some()
        || criteria.unseen.is_some()
        || criteria.flagged.is_some();
    if any {
        Ok(())
    } else {
        Err(MailError::Invalid("a search needs a criterion".into()))
    }
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
mod mail_manage_tests;
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

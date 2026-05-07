//! SMTP send via lettre.
//!
//! Each call builds a fresh `AsyncSmtpTransport` — same approach as
//! the IMAP module: simplicity over connection reuse for Phase 1.
//! SMTP submission is short-lived in practice so the cost is small.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use lettre::message::header::ContentType;
use lettre::message::{Attachment as LettreAttachment, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message as LettreMessage, Tokio1Executor};

use crate::mail::error::MailError;
use crate::mail::types::{Address, ConnectionSecurity, OutgoingMessage, SmtpConfig};

/// Send an outgoing message. Returns the Message-ID assigned by lettre,
/// useful for storing a reference locally and for threading follow-ups.
pub async fn send_message(
    config: &SmtpConfig,
    msg: &OutgoingMessage,
) -> Result<String, MailError> {
    let lettre_msg = build_lettre_message(msg)?;
    let message_id = lettre_msg
        .headers()
        .get_raw("Message-ID")
        .map(|s| s.trim_matches(|c: char| c == '<' || c == '>').to_string())
        .unwrap_or_default();

    let transport = build_transport(config)?;

    transport
        .send(lettre_msg)
        .await
        .map_err(|e| match (e.is_permanent(), e.is_transient()) {
            // Permanent failures often include auth errors (5.7.x).
            (true, _) if format!("{e}").to_ascii_lowercase().contains("authentication") => {
                MailError::SmtpAuth {
                    username: config.username.clone(),
                    reason: e.to_string(),
                }
            }
            _ => MailError::Smtp {
                reason: e.to_string(),
            },
        })?;

    Ok(message_id)
}

/// Build (but don't send) a message — exposed so the IMAP "save to
/// Sent folder" flow can append the same MIME bytes.
pub fn build_message_bytes(msg: &OutgoingMessage) -> Result<Vec<u8>, MailError> {
    let lettre_msg = build_lettre_message(msg)?;
    Ok(lettre_msg.formatted())
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn build_transport(
    config: &SmtpConfig,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, MailError> {
    let creds = Credentials::new(config.username.clone(), config.password.clone());

    let transport = match config.security {
        ConnectionSecurity::Tls => {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                .map_err(|e| MailError::Smtp {
                    reason: format!("relay({}): {e}", config.host),
                })?
                .port(config.port)
                .credentials(creds)
                .build()
        }
        ConnectionSecurity::StartTls => {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                .map_err(|e| MailError::Smtp {
                    reason: format!("starttls_relay({}): {e}", config.host),
                })?
                .port(config.port)
                .credentials(creds)
                .build()
        }
        ConnectionSecurity::None => {
            return Err(MailError::InvalidConfig {
                reason: "ConnectionSecurity::None is rejected for SMTP submission".to_string(),
            });
        }
    };

    Ok(transport)
}

fn build_lettre_message(msg: &OutgoingMessage) -> Result<LettreMessage, MailError> {
    let mut builder = LettreMessage::builder()
        .from(parse_mailbox(&msg.from)?)
        .subject(&msg.subject);

    for to in &msg.to {
        builder = builder.to(parse_mailbox(to)?);
    }
    for cc in &msg.cc {
        builder = builder.cc(parse_mailbox(cc)?);
    }
    for bcc in &msg.bcc {
        builder = builder.bcc(parse_mailbox(bcc)?);
    }
    if let Some(reply_to) = &msg.reply_to {
        builder = builder.reply_to(parse_mailbox(reply_to)?);
    }
    if let Some(in_reply_to) = &msg.in_reply_to {
        builder = builder.in_reply_to(format!("<{}>", in_reply_to));
    }
    if !msg.references.is_empty() {
        let refs = msg
            .references
            .iter()
            .map(|r| format!("<{}>", r))
            .collect::<Vec<_>>()
            .join(" ");
        builder = builder.references(refs);
    }

    let body = build_body(msg)?;
    builder.multipart(body).map_err(|e| MailError::MessageBuild {
        reason: e.to_string(),
    })
}

fn build_body(msg: &OutgoingMessage) -> Result<MultiPart, MailError> {
    let alternative = match (&msg.body_text, &msg.body_html) {
        (Some(text), Some(html)) => Some(
            MultiPart::alternative()
                .singlepart(SinglePart::plain(text.clone()))
                .singlepart(SinglePart::html(html.clone())),
        ),
        (Some(text), None) => Some(MultiPart::alternative().singlepart(SinglePart::plain(text.clone()))),
        (None, Some(html)) => Some(MultiPart::alternative().singlepart(SinglePart::html(html.clone()))),
        (None, None) => None,
    };

    if msg.attachments.is_empty() {
        return alternative.ok_or_else(|| MailError::MessageBuild {
            reason: "message has no body and no attachments".to_string(),
        });
    }

    // mixed = (alternative body) + (attachments...)
    let mut mixed = MultiPart::mixed().build();
    if let Some(alt) = alternative {
        mixed = mixed.multipart(alt);
    }

    for att in &msg.attachments {
        let bytes = STANDARD.decode(&att.data).map_err(|e| MailError::MessageBuild {
            reason: format!("base64-decode attachment '{}': {e}", att.filename),
        })?;
        let content_type = ContentType::parse(&att.content_type).map_err(|e| {
            MailError::MessageBuild {
                reason: format!("invalid content-type '{}': {e}", att.content_type),
            }
        })?;
        let part = match &att.content_id {
            Some(cid) => LettreAttachment::new_inline(cid.clone()).body(bytes, content_type),
            None => LettreAttachment::new(att.filename.clone()).body(bytes, content_type),
        };
        mixed = mixed.singlepart(part);
    }

    Ok(mixed)
}

fn parse_mailbox(addr: &Address) -> Result<lettre::message::Mailbox, MailError> {
    let s = match &addr.name {
        Some(name) => format!("{name} <{}>", addr.email),
        None => addr.email.clone(),
    };
    s.parse::<lettre::message::Mailbox>()
        .map_err(|e| MailError::MessageBuild {
            reason: format!("invalid address '{s}': {e}"),
        })
}

//! Building and sending messages (`lettre`, TLS through rustls with ring).

use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message as LettreMessage, Tokio1Executor};

use super::{check_server, Address, ConnectionSecurity, MailError, OutgoingMessage, ServerConfig};

/// How long one SMTP exchange may take.
const SMTP_TIMEOUT: Duration = Duration::from_secs(60);

/// Creates a validation/build error without exposing message content.
fn build_error(message: &str) -> MailError {
    MailError::Build(message.to_owned())
}

/// A header text the extension sends: no line break, so it cannot start another header.
fn single_line(text: &str) -> Result<&str, MailError> {
    if text.contains(['\r', '\n']) {
        return Err(build_error("header text with a line break"));
    }
    Ok(text)
}

/// A Message-ID without its angle brackets.
fn message_id(text: &str) -> Result<String, MailError> {
    let id = single_line(text)?.trim().trim_matches(['<', '>']);
    if id.is_empty() || id.contains(['<', '>', ' ']) {
        return Err(build_error("message id not allowed"));
    }
    Ok(format!("<{id}>"))
}

/// Converts and validates an SDK address for lettre.
fn mailbox(address: &Address) -> Result<Mailbox, MailError> {
    let email = single_line(&address.email)?
        .parse()
        .map_err(|_| build_error("address not allowed"))?;
    let name = address
        .name
        .as_deref()
        .map(single_line)
        .transpose()?
        .map(str::to_owned);
    Ok(Mailbox::new(name, email))
}

/// Builds the multipart body and validates all attachment metadata.
fn body(message: &OutgoingMessage) -> Result<MultiPart, MailError> {
    let alternative = match (&message.body_text, &message.body_html) {
        (None, None) => None,
        (text, html) => {
            let mut parts = MultiPart::alternative().build();
            if let Some(text) = text {
                parts = parts.singlepart(SinglePart::plain(text.clone()));
            }
            if let Some(html) = html {
                parts = parts.singlepart(SinglePart::html(html.clone()));
            }
            Some(parts)
        }
    };
    let mut inline = Vec::new();
    let mut regular = Vec::new();
    for attachment in &message.attachments {
        let bytes = STANDARD
            .decode(&attachment.data)
            .map_err(|_| build_error("attachment data must be base64"))?;
        let content_type = ContentType::parse(single_line(&attachment.content_type)?)
            .map_err(|_| build_error("attachment content type not allowed"))?;
        let filename = single_line(&attachment.filename)?.to_owned();
        match &attachment.content_id {
            Some(cid) => inline.push(
                Attachment::new_inline(single_line(cid)?.to_owned()).body(bytes, content_type),
            ),
            None => regular.push(Attachment::new(filename).body(bytes, content_type)),
        }
    }
    let with_inline = if inline.is_empty() {
        alternative
    } else {
        let mut related = MultiPart::related().build();
        if let Some(alternative) = alternative {
            related = related.multipart(alternative);
        }
        for part in inline {
            related = related.singlepart(part);
        }
        Some(related)
    };
    if regular.is_empty() {
        return with_inline.ok_or_else(|| build_error("a message needs a body or an attachment"));
    }
    let mut mixed = MultiPart::mixed().build();
    if let Some(body) = with_inline {
        mixed = mixed.multipart(body);
    }
    for part in regular {
        mixed = mixed.singlepart(part);
    }
    Ok(mixed)
}

/// A new Message-ID in the sender's domain; the device's host name never leaves it.
fn new_message_id(from: &Mailbox) -> String {
    format!("<{}@{}>", uuid::Uuid::new_v4(), from.email.domain())
}

/// The message as `lettre` sends it; `keep_bcc` for a draft, whose Bcc must survive APPEND.
fn build(message: &OutgoingMessage, keep_bcc: bool) -> Result<LettreMessage, MailError> {
    let from = mailbox(&message.from)?;
    let mut builder = LettreMessage::builder()
        // The sender gets the new Message-ID back.
        .message_id(Some(new_message_id(&from)))
        .from(from)
        .subject(single_line(&message.subject)?);
    if keep_bcc {
        builder = builder.keep_bcc();
    }
    for to in &message.to {
        builder = builder.to(mailbox(to)?);
    }
    for cc in &message.cc {
        builder = builder.cc(mailbox(cc)?);
    }
    for bcc in &message.bcc {
        builder = builder.bcc(mailbox(bcc)?);
    }
    if let Some(reply_to) = &message.reply_to {
        builder = builder.reply_to(mailbox(reply_to)?);
    }
    if let Some(in_reply_to) = &message.in_reply_to {
        builder = builder.in_reply_to(message_id(in_reply_to)?);
    }
    if !message.references.is_empty() {
        let references = message
            .references
            .iter()
            .map(|r| message_id(r))
            .collect::<Result<Vec<_>, _>>()?;
        builder = builder.references(references.join(" "));
    }
    builder
        .multipart(body(message)?)
        .map_err(|e| MailError::Build(e.to_string()))
}

/// The RFC 822 bytes of a message, without sending it (a draft for APPEND).
pub fn build_rfc822(message: &OutgoingMessage) -> Result<Vec<u8>, MailError> {
    Ok(build(message, true)?.formatted())
}

/// Builds the SMTP transport with the configured security and timeout.
fn transport(config: &ServerConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>, MailError> {
    check_server(&config.host, config.port, config.security)?;
    let builder = match config.security {
        ConnectionSecurity::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host),
        ConnectionSecurity::StartTls => {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
        }
        ConnectionSecurity::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
            &config.host,
        )),
    }
    .map_err(|error| MailError::Tls(format!("{}: {error}", config.host)))?
    .port(config.port)
    .timeout(Some(SMTP_TIMEOUT));
    let builder = if config.username.is_empty() {
        builder
    } else {
        builder.credentials(Credentials::new(
            config.username.clone(),
            config.password.clone(),
        ))
    };
    Ok(builder.build())
}

/// Sends a message; returns its Message-ID without angle brackets.
pub async fn send(config: &ServerConfig, message: &OutgoingMessage) -> Result<String, MailError> {
    let built = build(message, false)?;
    let id = built
        .headers()
        .get_raw("Message-ID")
        .map(|v| v.trim().trim_matches(['<', '>']).to_owned())
        .unwrap_or_default();
    transport(config)?.send(built).await.map_err(|e| {
        if e.is_permanent() && e.to_string().to_ascii_lowercase().contains("auth") {
            MailError::Auth
        } else {
            MailError::Smtp(if e.is_transient() {
                "the server is busy, try again later".into()
            } else {
                "the server refused the message".into()
            })
        }
    })?;
    Ok(id)
}

//! RFC822 → `Message` parsing via mail-parser.
//!
//! Splits the parsed MIME tree into a plain text part, an HTML part,
//! and attachment metadata. Body bytes for attachments are NOT included
//! to keep the response small — a follow-up fetch by `part_index`
//! delivers the data.

use async_imap::types::Fetch;
use mail_parser::{Address as ParsedAddress, MessageParser, MimeHeaders};

use crate::mail::error::MailError;
use crate::mail::types::{Address, Attachment, Message, MessageEnvelope};

/// Parse a fetched RFC822 message body into the public `Message` shape.
///
/// `fetch` provides envelope info (UID, flags, size, internal date)
/// that mail-parser doesn't see — IMAP delivers them out-of-band.
pub fn parse_message(rfc822: &[u8], fetch: &Fetch) -> Result<Message, MailError> {
    let parsed = MessageParser::default()
        .parse(rfc822)
        .ok_or_else(|| MailError::MessageParse {
            reason: "mail-parser returned None".to_string(),
        })?;

    let subject = parsed.subject().map(|s| s.to_string());

    let from: Vec<Address> = parsed
        .from()
        .map(addresses_from_header)
        .unwrap_or_default();
    let to: Vec<Address> = parsed.to().map(addresses_from_header).unwrap_or_default();
    let cc: Vec<Address> = parsed.cc().map(addresses_from_header).unwrap_or_default();

    let message_id = parsed.message_id().map(|s| s.to_string());
    let in_reply_to = parsed.in_reply_to().as_text().map(|s| s.to_string());
    let references: Vec<String> = parsed
        .references()
        .as_text_list()
        .map(|l| l.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let body_text = parsed.body_text(0).map(|c| c.into_owned());
    let body_html = parsed.body_html(0).map(|c| c.into_owned());

    let mut attachments = Vec::new();
    for (idx, part) in parsed.attachments().enumerate() {
        let filename = part.attachment_name().map(|s| s.to_string());
        let content_type = part
            .content_type()
            .map(|ct| match ct.subtype() {
                Some(sub) => format!("{}/{}", ct.ctype(), sub),
                None => ct.ctype().to_string(),
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let content_id = part.content_id().map(|s| s.to_string());
        let is_inline = part
            .content_disposition()
            .map(|cd| cd.is_inline())
            .unwrap_or(false);
        let size = part.contents().len() as u64;

        attachments.push(Attachment {
            part_index: idx as u32,
            filename,
            content_type,
            size,
            content_id,
            is_inline,
        });
    }

    let envelope = MessageEnvelope {
        uid: fetch.uid.unwrap_or(0),
        flags: fetch.flags().map(|f| format!("{:?}", f)).collect(),
        internal_date: fetch.internal_date().map(|d| d.timestamp()),
        subject,
        from,
        to,
        cc,
        message_id,
        in_reply_to,
        references,
        size: fetch.size,
    };

    Ok(Message {
        envelope,
        body_text,
        body_html,
        attachments,
    })
}

/// Read the supplementary `BODY.PEEK[HEADER.FIELDS (REFERENCES ...)]`
/// section of a FETCH response and split the References header.
pub fn extract_references_header(fetch: &Fetch) -> Vec<String> {
    let header = match fetch.header() {
        Some(bytes) => bytes,
        None => return Vec::new(),
    };
    let text = match std::str::from_utf8(header) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    // Extract the References: line (case-insensitive). Split into
    // <message-id> tokens — whitespace and angle brackets separate.
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("references:") {
            // Use the original line for case preservation.
            let original_rest = &line[line.len() - rest.len()..];
            return original_rest
                .split_whitespace()
                .map(|t| t.trim_matches(|c: char| c == '<' || c == '>'))
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
        }
    }
    Vec::new()
}

fn addresses_from_header(h: &ParsedAddress) -> Vec<Address> {
    h.iter()
        .map(|a| {
            let name = a.name().map(|s| s.to_string());
            let email = a.address().unwrap_or("").to_string();
            Address { name, email }
        })
        .collect()
}

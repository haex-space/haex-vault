//! Reads a `mailto:` link (RFC 6068) into the fields a mail extension needs (spec 063 research
//! R2). The one place for form, limits and refusal: the window passes the link on and shows what
//! comes back, and a link that is refused never reaches an extension.

use percent_encoding::percent_decode_str;
use serde::Serialize;
use ts_rs::TS;

use crate::error::{HolziError, Result};

const SCHEME: &str = "mailto:";

/// Largest link holzi reads (FR-008).
const MAX_LINK_BYTES: usize = 32 * 1024;
/// Most addresses in one of `to`, `cc` and `bcc`.
const MAX_RECIPIENTS: usize = 50;
const MAX_SUBJECT_CHARS: usize = 998;
const MAX_BODY_BYTES: usize = 16 * 1024;
/// Longest address (RFC 5321 path limit).
const MAX_ADDRESS_CHARS: usize = 254;

/// What a mail link asks for. Nothing else of the link is kept: header fields such as
/// `in-reply-to` or `x-…` are dropped, never passed on (FR-007).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "MailRequestView"
)]
pub struct MailRequest {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
}

fn invalid(reason: &str) -> HolziError {
    HolziError::OpenerLinkInvalid {
        reason: reason.to_owned(),
    }
}

/// Percent-decodes `text` as UTF-8. `percent_decode_str` leaves a broken `%xx` as it is, so the
/// escapes are checked first.
fn decode(text: &str) -> Result<String> {
    let bytes = text.as_bytes();
    for (at, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && !bytes
                .get(at + 1..at + 3)
                .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit))
        {
            return Err(invalid("escape"));
        }
    }
    percent_decode_str(text)
        .decode_utf8()
        .map(std::borrow::Cow::into_owned)
        .map_err(|_| invalid("utf8"))
}

/// One address: not empty, at most 254 characters, exactly one `@` with something on both sides,
/// no whitespace or control characters. The extension checks its addresses itself.
fn check_address(address: &str) -> Result<()> {
    let well_formed = address.chars().count() <= MAX_ADDRESS_CHARS
        && address.matches('@').count() == 1
        && address
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && !domain.is_empty())
        && !address.chars().any(|c| c.is_whitespace() || c.is_control());
    if well_formed {
        Ok(())
    } else {
        Err(invalid("address"))
    }
}

/// Appends the addresses of `text` (comma separated, each percent-encoded) to `into`. Empty
/// entries, as in `a@x,` or `to=`, are skipped.
fn add_addresses(into: &mut Vec<String>, text: &str) -> Result<()> {
    for part in text.split(',').filter(|part| !part.is_empty()) {
        let address = decode(part)?;
        if address.contains(['\r', '\n']) {
            return Err(invalid("line_break"));
        }
        check_address(&address)?;
        into.push(address);
    }
    if into.len() > MAX_RECIPIENTS {
        return Err(HolziError::OpenerLinkTooLarge);
    }
    Ok(())
}

/// Reads `link`. A link without recipients is valid (`mailto:?subject=…`); the first `subject` and
/// the first `body` count.
pub fn parse(link: &str) -> Result<MailRequest> {
    if link.len() > MAX_LINK_BYTES {
        return Err(HolziError::OpenerLinkTooLarge);
    }
    let rest = link
        .get(..SCHEME.len())
        .filter(|scheme| scheme.eq_ignore_ascii_case(SCHEME))
        .map(|_| &link[SCHEME.len()..])
        .ok_or_else(|| invalid("scheme"))?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));

    let mut request = MailRequest::default();
    let mut subject = None;
    let mut body = None;
    add_addresses(&mut request.to, path)?;
    for pair in query.split('&') {
        let Some((name, value)) = pair.split_once('=') else {
            continue;
        };
        let name = decode(name)?.to_ascii_lowercase();
        let value = decode(value)?;
        match name.as_str() {
            "to" => add_addresses(&mut request.to, &value)?,
            "cc" => add_addresses(&mut request.cc, &value)?,
            "bcc" => add_addresses(&mut request.bcc, &value)?,
            "subject" if subject.is_none() => subject = Some(value),
            "body" if body.is_none() => body = Some(value),
            _ => {}
        }
    }

    request.subject = subject.unwrap_or_default();
    if request.subject.contains(['\r', '\n']) {
        return Err(invalid("line_break"));
    }
    if request.subject.chars().count() > MAX_SUBJECT_CHARS {
        return Err(HolziError::OpenerLinkTooLarge);
    }
    request.body = body.unwrap_or_default().replace("\r\n", "\n");
    if request.body.len() > MAX_BODY_BYTES {
        return Err(HolziError::OpenerLinkTooLarge);
    }
    Ok(request)
}

#[cfg(test)]
#[path = "mail_tests.rs"]
mod mail_tests;

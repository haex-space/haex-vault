//! IMAP fetch operations.
//!
//! Phase 1 scope: implicit TLS only (port 993). STARTTLS and plain
//! connections return `MailError::InvalidConfig` — they can be added
//! later without breaking the public API.
//!
//! Each public function connects + logs in + does its work + logs out.
//! No connection pooling yet (see module docs).

use async_imap::types::Fetch;
use async_imap::Session;
use futures_util::TryStreamExt;
use imap_proto::types::{Address as ImapAddress, BodyStructure};
use mail_parser::{parsers::MessageStream, HeaderValue};
use tokio::net::TcpStream;
use tokio_native_tls::TlsStream;
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

use crate::mail::error::MailError;
use crate::mail::parsing;
use crate::mail::types::{
    Address, ConnectionSecurity, FetchRange, ImapConfig, MailboxInfo, Message, MessageEnvelope,
};

type ImapStream = Compat<TlsStream<TcpStream>>;
type ImapSession = Session<ImapStream>;

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// Open an IMAPS connection and authenticate.
///
/// On success the caller owns the `Session` and is responsible for
/// calling `logout` (best-effort) before drop.
async fn login(config: &ImapConfig) -> Result<ImapSession, MailError> {
    if !matches!(config.security, ConnectionSecurity::Tls) {
        return Err(MailError::InvalidConfig {
            reason: "Phase 1 only supports ConnectionSecurity::Tls (implicit TLS, port 993)"
                .to_string(),
        });
    }

    let tcp = TcpStream::connect((config.host.as_str(), config.port))
        .await
        .map_err(|e| MailError::Connect {
            host: config.host.clone(),
            port: config.port,
            source: e,
        })?;

    let connector_inner =
        native_tls::TlsConnector::builder()
            .build()
            .map_err(|e| MailError::Tls {
                host: config.host.clone(),
                reason: format!("connector build: {e}"),
            })?;
    let connector = tokio_native_tls::TlsConnector::from(connector_inner);

    let tls = connector
        .connect(&config.host, tcp)
        .await
        .map_err(|e| MailError::Tls {
            host: config.host.clone(),
            reason: e.to_string(),
        })?;

    let client = async_imap::Client::new(tls.compat());

    let session = client
        .login(&config.username, &config.password)
        .await
        .map_err(|(e, _client)| MailError::ImapAuth {
            username: config.username.clone(),
            reason: e.to_string(),
        })?;

    Ok(session)
}

/// Best-effort logout. Errors are swallowed because failures here are
/// recoverable on the next connect.
async fn logout(mut session: ImapSession) {
    let _ = session.logout().await;
}

// ---------------------------------------------------------------------------
// Public operations
// ---------------------------------------------------------------------------

/// LIST mailboxes + STATUS UIDVALIDITY/UIDNEXT/MESSAGES/UNSEEN per box.
///
/// `reference` and `pattern` map to IMAP LIST args; defaults `("", "*")`
/// return the full hierarchy.
pub async fn list_mailboxes(
    config: &ImapConfig,
    reference: Option<&str>,
    pattern: Option<&str>,
    include_status: bool,
) -> Result<Vec<MailboxInfo>, MailError> {
    let mut session = login(config).await?;
    let result = list_mailboxes_inner(&mut session, reference, pattern, include_status).await;
    logout(session).await;
    result
}

async fn list_mailboxes_inner(
    session: &mut ImapSession,
    reference: Option<&str>,
    pattern: Option<&str>,
    include_status: bool,
) -> Result<Vec<MailboxInfo>, MailError> {
    let reference = reference.unwrap_or("");
    let pattern = pattern.unwrap_or("*");

    let names = session
        .list(Some(reference), Some(pattern))
        .await
        .map_err(imap_err)?;

    let raw: Vec<async_imap::types::Name> = names.try_collect().await.map_err(imap_err)?;
    // NameAttribute (mailbox-list flags like \HasNoChildren) is a
    // separate type from Flag, so we keep the Debug rendering here.
    let mut entries: Vec<MailboxInfo> = raw
        .iter()
        .map(|name| MailboxInfo {
            name: name.name().to_string(),
            delimiter: name.delimiter().map(|s| s.to_string()),
            flags: name
                .attributes()
                .iter()
                .map(|f| format!("{:?}", f))
                .collect(),
            exists: None,
            unseen: None,
            uid_validity: None,
            uid_next: None,
        })
        .collect();

    if include_status {
        // STATUS is one round-trip per mailbox. Acceptable for typical
        // accounts with <50 folders; large IMAP trees should pass
        // include_status=false and request status lazily.
        for info in entries.iter_mut() {
            if let Ok(status) = session
                .status(&info.name, "(MESSAGES UNSEEN UIDVALIDITY UIDNEXT)")
                .await
            {
                info.exists = Some(status.exists);
                info.unseen = status.unseen;
                info.uid_validity = status.uid_validity;
                info.uid_next = status.uid_next;
            }
        }
    }

    Ok(entries)
}

/// SELECT a mailbox and fetch envelopes for the given range.
///
/// Always uses UID-based fetch — sequence numbers are unstable across
/// EXPUNGEs, UIDs are not.
pub async fn fetch_envelopes(
    config: &ImapConfig,
    mailbox: &str,
    range: &FetchRange,
) -> Result<Vec<MessageEnvelope>, MailError> {
    let mut session = login(config).await?;
    let result = fetch_envelopes_inner(&mut session, mailbox, range).await;
    logout(session).await;
    result
}

async fn fetch_envelopes_inner(
    session: &mut ImapSession,
    mailbox: &str,
    range: &FetchRange,
) -> Result<Vec<MessageEnvelope>, MailError> {
    let mailbox_meta = session.select(mailbox).await.map_err(imap_err)?;

    let uid_set = build_uid_set(session, &mailbox_meta, range).await?;
    if uid_set.is_empty() {
        return Ok(Vec::new());
    }

    // RFC822.SIZE + INTERNALDATE + FLAGS + ENVELOPE for the summary;
    // BODYSTRUCTURE (structure only, no part data) to derive
    // `has_attachments` cheaply for the list view; BODY.PEEK[HEADER.FIELDS
    // (...)] for threading headers without marking the message \Seen.
    let query = "(UID FLAGS INTERNALDATE RFC822.SIZE ENVELOPE BODYSTRUCTURE \
                 BODY.PEEK[HEADER.FIELDS (MESSAGE-ID IN-REPLY-TO REFERENCES)])";

    let stream = session.uid_fetch(&uid_set, query).await.map_err(imap_err)?;
    let fetches: Vec<Fetch> = stream.try_collect().await.map_err(imap_err)?;

    let mut out = Vec::with_capacity(fetches.len());
    for f in &fetches {
        out.push(envelope_from_fetch(f));
    }
    Ok(out)
}

/// Fetch a single full message (envelope + parsed body parts +
/// attachment metadata). Use `fetch_envelopes` for list views.
pub async fn fetch_message(
    config: &ImapConfig,
    mailbox: &str,
    uid: u32,
) -> Result<Message, MailError> {
    let mut session = login(config).await?;
    let result = fetch_message_inner(&mut session, mailbox, uid).await;
    logout(session).await;
    result
}

async fn fetch_message_inner(
    session: &mut ImapSession,
    mailbox: &str,
    uid: u32,
) -> Result<Message, MailError> {
    session.select(mailbox).await.map_err(imap_err)?;

    let stream = session
        .uid_fetch(
            uid.to_string(),
            "(UID FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[])",
        )
        .await
        .map_err(imap_err)?;

    let fetches: Vec<Fetch> = stream.try_collect().await.map_err(imap_err)?;

    let fetch = fetches.first().ok_or_else(|| MailError::Imap {
        reason: format!("no message with UID {} in mailbox '{}'", uid, mailbox),
    })?;

    let body = fetch.body().ok_or_else(|| MailError::Imap {
        reason: "FETCH returned no BODY data".to_string(),
    })?;

    let parsed = parsing::parse_message(body, fetch)?;
    Ok(parsed)
}

/// Fetch the raw bytes of a single attachment by `part_index`. Re-fetches
/// the full message (BODY.PEEK[], like `fetch_message`) and returns the
/// bytes of the addressed MIME part — used for opening/downloading and for
/// re-attaching when forwarding.
pub async fn fetch_attachment(
    config: &ImapConfig,
    mailbox: &str,
    uid: u32,
    part_index: u32,
) -> Result<Vec<u8>, MailError> {
    let mut session = login(config).await?;
    let result = fetch_attachment_inner(&mut session, mailbox, uid, part_index).await;
    logout(session).await;
    result
}

async fn fetch_attachment_inner(
    session: &mut ImapSession,
    mailbox: &str,
    uid: u32,
    part_index: u32,
) -> Result<Vec<u8>, MailError> {
    session.select(mailbox).await.map_err(imap_err)?;

    let stream = session
        .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
        .await
        .map_err(imap_err)?;

    let fetches: Vec<Fetch> = stream.try_collect().await.map_err(imap_err)?;

    let fetch = fetches.first().ok_or_else(|| MailError::Imap {
        reason: format!("no message with UID {} in mailbox '{}'", uid, mailbox),
    })?;

    let body = fetch.body().ok_or_else(|| MailError::Imap {
        reason: "FETCH returned no BODY data".to_string(),
    })?;

    parsing::extract_attachment(body, part_index)
}

/// Set or unset flags on a UID set. `add=true` for STORE +FLAGS,
/// `add=false` for STORE -FLAGS. Use `flags=["\\Seen"]` to mark read.
pub async fn set_flags(
    config: &ImapConfig,
    mailbox: &str,
    uids: &[u32],
    flags: &[String],
    add: bool,
) -> Result<(), MailError> {
    if uids.is_empty() || flags.is_empty() {
        return Ok(());
    }
    let mut session = login(config).await?;
    let result = set_flags_inner(&mut session, mailbox, uids, flags, add).await;
    logout(session).await;
    result
}

async fn set_flags_inner(
    session: &mut ImapSession,
    mailbox: &str,
    uids: &[u32],
    flags: &[String],
    add: bool,
) -> Result<(), MailError> {
    session.select(mailbox).await.map_err(imap_err)?;

    let uid_set = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let op = if add { "+FLAGS" } else { "-FLAGS" };
    let flag_list = flags.join(" ");
    let query = format!("{op} ({flag_list})");

    let stream = session.uid_store(uid_set, query).await.map_err(imap_err)?;
    // Drain the response stream; the per-message echoes aren't needed,
    // but we propagate any per-item Err so a partial failure surfaces.
    let _: Vec<_> = stream.try_collect().await.map_err(imap_err)?;
    Ok(())
}

/// Move messages between mailboxes. Falls back to COPY+EXPUNGE if the
/// server doesn't advertise the MOVE extension.
pub async fn move_messages(
    config: &ImapConfig,
    source_mailbox: &str,
    destination_mailbox: &str,
    uids: &[u32],
) -> Result<(), MailError> {
    if uids.is_empty() {
        return Ok(());
    }
    let mut session = login(config).await?;
    let result = move_messages_inner(&mut session, source_mailbox, destination_mailbox, uids).await;
    logout(session).await;
    result
}

async fn move_messages_inner(
    session: &mut ImapSession,
    source_mailbox: &str,
    destination_mailbox: &str,
    uids: &[u32],
) -> Result<(), MailError> {
    session.select(source_mailbox).await.map_err(imap_err)?;
    let uid_set = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    // Try MOVE first; if the server returns BAD/NO we fall back to
    // COPY + STORE \Deleted + EXPUNGE. If the fallback itself fails we
    // surface the original MOVE error alongside the fallback error, so the
    // caller can diagnose which step actually broke (previously the MOVE
    // error was discarded with `Err(_)`).
    match session.uid_mv(&uid_set, destination_mailbox).await {
        Ok(()) => Ok(()),
        Err(mv_err) => {
            let fallback = async {
                session
                    .uid_copy(&uid_set, destination_mailbox)
                    .await
                    .map_err(imap_err)?;
                let stream = session
                    .uid_store(&uid_set, "+FLAGS (\\Deleted)")
                    .await
                    .map_err(imap_err)?;
                let _: Vec<_> = stream.try_collect().await.map_err(imap_err)?;
                // UID EXPUNGE only removes the UIDs we just marked \Deleted,
                // so concurrent \Deleted flags from other clients survive.
                let expunge_stream = session.uid_expunge(&uid_set).await.map_err(imap_err)?;
                let _: Vec<_> = expunge_stream.try_collect().await.map_err(imap_err)?;
                Ok::<(), MailError>(())
            }
            .await;

            match fallback {
                Ok(()) => Ok(()),
                Err(fb_err) => Err(MailError::Imap {
                    reason: format!(
                        "MOVE failed ({mv_err}) and COPY+EXPUNGE fallback also failed: {fb_err}"
                    ),
                }),
            }
        }
    }
}

/// APPEND a raw RFC822 message into a mailbox (typically used to save
/// a copy of a sent message into "Sent").
pub async fn append_message(
    config: &ImapConfig,
    mailbox: &str,
    rfc822: &[u8],
    flags: &[String],
) -> Result<(), MailError> {
    let mut session = login(config).await?;
    let result = append_message_inner(&mut session, mailbox, rfc822, flags).await;
    logout(session).await;
    result
}

async fn append_message_inner(
    session: &mut ImapSession,
    mailbox: &str,
    rfc822: &[u8],
    flags: &[String],
) -> Result<(), MailError> {
    // The wire format for flags is "(\\Flag1 \\Flag2)" — including the
    // parens. Empty flag list → pass None to omit the parameter.
    let flags_arg = if flags.is_empty() {
        None
    } else {
        Some(format!("({})", flags.join(" ")))
    };
    session
        .append(mailbox, flags_arg.as_deref(), None, rfc822)
        .await
        .map_err(imap_err)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn imap_err(e: async_imap::error::Error) -> MailError {
    MailError::Imap {
        reason: e.to_string(),
    }
}

/// Decode an RFC 2047 encoded-word header value (`=?charset?enc?data?=`).
///
/// The IMAP ENVELOPE response returns header fields (subject, address
/// display-names) verbatim off the wire — the server does not decode
/// MIME encoded-words, unlike `parsing::parse_message`'s mail-parser-based
/// full-body parse. Falls back to lossy UTF-8 for plain, unencoded values.
fn decode_rfc2047(raw: &[u8]) -> String {
    // `parse_unstructured` only flushes its token buffer on a header-line
    // terminator — raw ENVELOPE field bytes don't carry one.
    let mut header = Vec::with_capacity(raw.len() + 1);
    header.extend_from_slice(raw);
    header.push(b'\n');
    match MessageStream::new(&header).parse_unstructured() {
        HeaderValue::Text(s) => s.into_owned(),
        _ => String::from_utf8_lossy(raw).into_owned(),
    }
}

/// Build a UID-set string for IMAP from a `FetchRange`.
///
/// `FetchRange::Latest` is translated by querying the current EXISTS
/// from the SELECT response and computing `last-N+1:last`. Note the
/// server returns this in chronological order — newest LAST — callers
/// who want newest-first must reverse the result.
async fn build_uid_set(
    session: &mut ImapSession,
    mailbox_meta: &async_imap::types::Mailbox,
    range: &FetchRange,
) -> Result<String, MailError> {
    match range {
        FetchRange::Latest { count } => {
            let exists = mailbox_meta.exists;
            if exists == 0 || *count == 0 {
                return Ok(String::new());
            }
            let take = (*count).min(exists);
            let start_seq = exists - take + 1;

            // Convert sequence range → UID list via `UID SEARCH <seq-set>`.
            // `session.search()` would return sequence numbers, which would
            // then be misinterpreted as UIDs by the caller's `uid_fetch`.
            let seq_set = format!("{}:{}", start_seq, exists);
            let uids = session.uid_search(seq_set).await.map_err(imap_err)?;
            if uids.is_empty() {
                return Ok(String::new());
            }
            Ok(uids
                .into_iter()
                .map(|u| u.to_string())
                .collect::<Vec<_>>()
                .join(","))
        }
        FetchRange::UidRange { start, end } => Ok(format!("{}:{}", start, end)),
        FetchRange::UidList { uids } => Ok(uids
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",")),
    }
}

fn envelope_from_fetch(f: &Fetch) -> MessageEnvelope {
    let uid = f.uid.unwrap_or(0);
    let internal_date = f.internal_date().map(|d| d.timestamp());
    let size = f.size;
    let flags: Vec<String> = f.flags().map(|fl| parsing::flag_to_string(&fl)).collect();

    let env = f.envelope();
    let subject = env
        .as_ref()
        .and_then(|e| e.subject.as_ref())
        .map(|b| decode_rfc2047(b));

    let from = env
        .as_ref()
        .and_then(|e| e.from.as_ref())
        .map(|list| list.iter().map(addr_from_imap).collect())
        .unwrap_or_default();
    let to = env
        .as_ref()
        .and_then(|e| e.to.as_ref())
        .map(|list| list.iter().map(addr_from_imap).collect())
        .unwrap_or_default();
    let cc = env
        .as_ref()
        .and_then(|e| e.cc.as_ref())
        .map(|list| list.iter().map(addr_from_imap).collect())
        .unwrap_or_default();

    let message_id = env
        .as_ref()
        .and_then(|e| e.message_id.as_ref())
        .map(|b| String::from_utf8_lossy(b).into_owned());
    let in_reply_to = env
        .as_ref()
        .and_then(|e| e.in_reply_to.as_ref())
        .map(|b| String::from_utf8_lossy(b).into_owned());

    // References header is in the supplementary BODY.PEEK[HEADER.FIELDS ...].
    let references = parsing::extract_references_header(f);

    let has_attachments = f.bodystructure().map(has_non_body_part).unwrap_or(false);

    MessageEnvelope {
        uid,
        flags,
        internal_date,
        subject,
        from,
        to,
        cc,
        message_id,
        in_reply_to,
        references,
        size,
        has_attachments,
    }
}

/// Whether a BODYSTRUCTURE contains any part beyond a plain single
/// text body or a text/plain+text/html alternative pair — i.e. a
/// regular attachment or an inline (cid-referenced) part. Mirrors what
/// `parsing::parse_message`'s mail-parser-based `.attachments()` counts
/// for a fully fetched message, so the list-view flag and the opened
/// message's attachment list agree.
fn has_non_body_part(bs: &BodyStructure<'_>) -> bool {
    match bs {
        // A basic (non-text, non-message) part is never the message's
        // primary body — always attachment/inline content.
        BodyStructure::Basic { .. } => true,
        // An embedded RFC822 message (forwarded mail) is likewise never
        // the primary body.
        BodyStructure::Message { .. } => true,
        BodyStructure::Text { common, .. } => common
            .disposition
            .as_ref()
            .is_some_and(|d| d.ty.eq_ignore_ascii_case("attachment")),
        BodyStructure::Multipart { bodies, .. } => bodies.iter().any(has_non_body_part),
    }
}

fn addr_from_imap(a: &ImapAddress) -> Address {
    let name = a.name.as_ref().map(|b| decode_rfc2047(b));
    let mailbox = a
        .mailbox
        .as_ref()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    let host = a
        .host
        .as_ref()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();

    let email = if host.is_empty() {
        mailbox
    } else {
        format!("{mailbox}@{host}")
    };
    Address { name, email }
}

#[cfg(test)]
mod tests {
    use super::*;
    use imap_proto::types::{
        BodyContentCommon, BodyContentSinglePart, ContentDisposition, ContentEncoding, ContentType,
        Envelope,
    };

    fn text_part(subtype: &str, disposition: Option<&str>) -> BodyStructure<'static> {
        BodyStructure::Text {
            common: BodyContentCommon {
                ty: ContentType {
                    ty: "text".into(),
                    subtype: subtype.to_string().into(),
                    params: None,
                },
                disposition: disposition.map(|d| ContentDisposition {
                    ty: d.to_string().into(),
                    params: None,
                }),
                language: None,
                location: None,
            },
            other: BodyContentSinglePart {
                id: None,
                md5: None,
                description: None,
                transfer_encoding: ContentEncoding::SevenBit,
                octets: 0,
            },
            lines: 0,
            extension: None,
        }
    }

    fn basic_part(ty: &str, subtype: &str) -> BodyStructure<'static> {
        BodyStructure::Basic {
            common: BodyContentCommon {
                ty: ContentType {
                    ty: ty.to_string().into(),
                    subtype: subtype.to_string().into(),
                    params: None,
                },
                disposition: None,
                language: None,
                location: None,
            },
            other: BodyContentSinglePart {
                id: None,
                md5: None,
                description: None,
                transfer_encoding: ContentEncoding::Base64,
                octets: 0,
            },
            extension: None,
        }
    }

    fn embedded_message_part() -> BodyStructure<'static> {
        BodyStructure::Message {
            common: BodyContentCommon {
                ty: ContentType {
                    ty: "message".into(),
                    subtype: "rfc822".into(),
                    params: None,
                },
                disposition: None,
                language: None,
                location: None,
            },
            other: BodyContentSinglePart {
                id: None,
                md5: None,
                description: None,
                transfer_encoding: ContentEncoding::SevenBit,
                octets: 0,
            },
            envelope: Envelope {
                date: None,
                subject: None,
                from: None,
                sender: None,
                reply_to: None,
                to: None,
                cc: None,
                bcc: None,
                in_reply_to: None,
                message_id: None,
            },
            body: Box::new(text_part("plain", None)),
            lines: 0,
            extension: None,
        }
    }

    fn multipart(subtype: &str, bodies: Vec<BodyStructure<'static>>) -> BodyStructure<'static> {
        BodyStructure::Multipart {
            common: BodyContentCommon {
                ty: ContentType {
                    ty: "multipart".into(),
                    subtype: subtype.to_string().into(),
                    params: None,
                },
                disposition: None,
                language: None,
                location: None,
            },
            bodies,
            extension: None,
        }
    }

    #[test]
    fn single_text_part_has_no_attachments() {
        assert!(!has_non_body_part(&text_part("plain", None)));
    }

    #[test]
    fn text_plus_html_alternative_has_no_attachments() {
        let bs = multipart(
            "alternative",
            vec![text_part("plain", None), text_part("html", None)],
        );
        assert!(!has_non_body_part(&bs));
    }

    #[test]
    fn mixed_with_file_part_has_attachments() {
        let bs = multipart(
            "mixed",
            vec![text_part("plain", None), basic_part("application", "pdf")],
        );
        assert!(has_non_body_part(&bs));
    }

    #[test]
    fn text_part_with_attachment_disposition_counts() {
        // Some servers report an attached .txt file as a Text bodystructure
        // node rather than Basic — disposition is the authoritative signal.
        assert!(has_non_body_part(&text_part("plain", Some("attachment"))));
    }

    #[test]
    fn nested_multipart_related_with_inline_image_has_attachments() {
        let bs = multipart(
            "mixed",
            vec![multipart(
                "related",
                vec![text_part("html", None), basic_part("image", "png")],
            )],
        );
        assert!(has_non_body_part(&bs));
    }

    #[test]
    fn embedded_message_counts_as_attachment() {
        // A forwarded email (message/rfc822 part) is never the primary body.
        let bs = multipart(
            "mixed",
            vec![text_part("plain", None), embedded_message_part()],
        );
        assert!(has_non_body_part(&bs));
    }

    #[test]
    fn decode_rfc2047_decodes_base64_encoded_word() {
        // ENVELOPE subject bytes as returned raw by the IMAP server.
        assert_eq!(
            decode_rfc2047(b"=?UTF-8?B?SGVsbG8gV8OWUkxE?="),
            "Hello WÖRLD",
        );
    }

    #[test]
    fn decode_rfc2047_passes_through_plain_ascii() {
        assert_eq!(
            decode_rfc2047(b"Plain ASCII subject"),
            "Plain ASCII subject"
        );
    }
}

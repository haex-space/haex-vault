//! IMAP operations. Each one logs in, does its work and logs out; there is no connection pool.

use std::collections::HashSet;

use async_imap::types::Fetch;
use futures::TryStreamExt;
use imap_proto::types::{Address as ImapAddress, BodyStructure, NameAttribute};
use mail_parser::{parsers::MessageStream, HeaderValue};

use super::connect::{imap_login, imap_logout, ImapSession};
use super::parsing;
use super::{
    check_flag, check_mailbox, Address, FetchRange, MailError, MailboxInfo, Message,
    MessageEnvelope, ServerConfig,
};

/// Converts an async-IMAP protocol error into the mail boundary error.
pub(super) fn imap_err(error: async_imap::error::Error) -> MailError {
    MailError::Imap(error.to_string())
}

/// Logs in, runs `work`, logs out.
macro_rules! with_session {
    ($config:expr, |$session:ident| $work:expr) => {{
        let mut $session = imap_login($config).await?;
        let result = $work;
        imap_logout($session).await;
        result
    }};
}
pub(super) use with_session;

/// The most envelopes one `fetchEnvelopes` returns; a larger range is refused, the extension asks
/// in pages.
pub const MAX_ENVELOPES: usize = 1000;

/// A LIST attribute as IMAP writes it (`\Noselect`, `\Sent`, …), which the SDK compares against.
fn name_attribute(attribute: &NameAttribute<'_>) -> String {
    match attribute {
        NameAttribute::NoInferiors => "\\Noinferiors".into(),
        NameAttribute::NoSelect => "\\Noselect".into(),
        NameAttribute::Marked => "\\Marked".into(),
        NameAttribute::Unmarked => "\\Unmarked".into(),
        NameAttribute::All => "\\All".into(),
        NameAttribute::Archive => "\\Archive".into(),
        NameAttribute::Drafts => "\\Drafts".into(),
        NameAttribute::Flagged => "\\Flagged".into(),
        NameAttribute::Junk => "\\Junk".into(),
        NameAttribute::Sent => "\\Sent".into(),
        NameAttribute::Trash => "\\Trash".into(),
        NameAttribute::Extension(name) => name.to_string(),
        other => format!("{other:?}"),
    }
}

/// A Message-ID without its angle brackets, as the other answers give it.
fn bare_id(id: String) -> String {
    id.trim().trim_matches(['<', '>']).to_owned()
}

/// Formats UIDs as the comma-separated IMAP set used by commands.
pub(super) fn uid_list(uids: &[u32]) -> String {
    uids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// LIST and, if asked, STATUS of every mailbox.
pub async fn list_mailboxes(
    config: &ServerConfig,
    reference: Option<&str>,
    pattern: Option<&str>,
    include_status: bool,
) -> Result<Vec<MailboxInfo>, MailError> {
    for text in [reference, pattern].into_iter().flatten() {
        if text.chars().any(char::is_control) {
            return Err(MailError::Invalid(
                "reference or pattern not allowed".into(),
            ));
        }
    }
    with_session!(config, |session| {
        list_inner(&mut session, reference, pattern, include_status).await
    })
}

/// Runs LIST and optional STATUS operations on an already authenticated session.
async fn list_inner(
    session: &mut ImapSession,
    reference: Option<&str>,
    pattern: Option<&str>,
    include_status: bool,
) -> Result<Vec<MailboxInfo>, MailError> {
    let names: Vec<async_imap::types::Name> = session
        .list(Some(reference.unwrap_or("")), Some(pattern.unwrap_or("*")))
        .await
        .map_err(imap_err)?
        .try_collect()
        .await
        .map_err(imap_err)?;
    let mut boxes: Vec<MailboxInfo> = names
        .iter()
        .map(|name| MailboxInfo {
            name: name.name().to_owned(),
            delimiter: name.delimiter().map(str::to_owned),
            flags: name.attributes().iter().map(name_attribute).collect(),
            exists: None,
            unseen: None,
            uid_validity: None,
            uid_next: None,
        })
        .collect();
    if include_status {
        for info in &mut boxes {
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
    Ok(boxes)
}

/// The UIDs a range names; `Latest` asks the server which UIDs the last messages have.
async fn uid_set(
    session: &mut ImapSession,
    exists: u32,
    range: &FetchRange,
) -> Result<String, MailError> {
    Ok(match range {
        FetchRange::Latest { count } => {
            if exists == 0 || *count == 0 {
                return Ok(String::new());
            }
            if *count as usize > MAX_ENVELOPES {
                return Err(MailError::TooLarge);
            }
            let start = exists - (*count).min(exists) + 1;
            let uids: HashSet<u32> = session
                .uid_search(format!("{start}:{exists}"))
                .await
                .map_err(imap_err)?;
            let mut uids: Vec<u32> = uids.into_iter().collect();
            uids.sort_unstable();
            uid_list(&uids)
        }
        FetchRange::UidRange { start, end } if *start > 0 && start <= end => {
            // The range may be sparse: the messages in it count, not its width.
            let uids: HashSet<u32> = session
                .uid_search(format!("UID {start}:{end}"))
                .await
                .map_err(imap_err)?;
            if uids.len() > MAX_ENVELOPES {
                return Err(MailError::TooLarge);
            }
            let mut uids: Vec<u32> = uids.into_iter().collect();
            uids.sort_unstable();
            uid_list(&uids)
        }
        FetchRange::UidRange { .. } => {
            return Err(MailError::Invalid("uid range not allowed".into()))
        }
        FetchRange::UidList { uids } if uids.len() > MAX_ENVELOPES => {
            return Err(MailError::TooLarge)
        }
        FetchRange::UidList { uids } => uid_list(uids),
    })
}

/// The envelopes of a range of a mailbox, without bodies.
pub async fn fetch_envelopes(
    config: &ServerConfig,
    mailbox: &str,
    range: &FetchRange,
) -> Result<Vec<MessageEnvelope>, MailError> {
    check_mailbox(mailbox)?;
    with_session!(config, |session| {
        async {
            let selected = session.select(mailbox).await.map_err(imap_err)?;
            let set = uid_set(&mut session, selected.exists, range).await?;
            if set.is_empty() {
                return Ok(Vec::new());
            }
            let fetches: Vec<Fetch> = session
                .uid_fetch(
                    &set,
                    "(UID FLAGS INTERNALDATE RFC822.SIZE ENVELOPE BODYSTRUCTURE \
                     BODY.PEEK[HEADER.FIELDS (MESSAGE-ID IN-REPLY-TO REFERENCES)])",
                )
                .await
                .map_err(imap_err)?
                .try_collect()
                .await
                .map_err(imap_err)?;
            Ok(fetches.iter().map(envelope_of).collect())
        }
        .await
    })
}

/// The whole message `uid`, at most `max_bytes` long; its size is asked for first, and the body is
/// fetched only up to one byte past the limit, so a server that reported a wrong size cannot send
/// more.
async fn fetch_body(
    session: &mut ImapSession,
    mailbox: &str,
    uid: u32,
    max_bytes: usize,
) -> Result<Fetch, MailError> {
    session.select(mailbox).await.map_err(imap_err)?;
    let sizes: Vec<Fetch> = session
        .uid_fetch(uid.to_string(), "(UID RFC822.SIZE)")
        .await
        .map_err(imap_err)?
        .try_collect()
        .await
        .map_err(imap_err)?;
    let size = sizes
        .iter()
        .find(|f| f.uid == Some(uid))
        .ok_or(MailError::NotFound)?
        .size
        .ok_or_else(|| MailError::Imap("no size in the answer".into()))?;
    if size as usize > max_bytes {
        return Err(MailError::TooLarge);
    }
    let fetches: Vec<Fetch> = session
        .uid_fetch(
            uid.to_string(),
            format!(
                "(UID FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[]<0.{}>)",
                max_bytes.saturating_add(1)
            ),
        )
        .await
        .map_err(imap_err)?
        .try_collect()
        .await
        .map_err(imap_err)?;
    let fetch = fetches
        .into_iter()
        .find(|f| f.uid == Some(uid))
        .ok_or(MailError::NotFound)?;
    if fetch.body().map_or(0, <[u8]>::len) > max_bytes {
        return Err(MailError::TooLarge);
    }
    Ok(fetch)
}

/// One whole message: envelope, text and HTML body, attachment metadata.
pub async fn fetch_message(
    config: &ServerConfig,
    mailbox: &str,
    uid: u32,
    max_bytes: usize,
) -> Result<Message, MailError> {
    check_mailbox(mailbox)?;
    with_session!(config, |session| {
        async {
            let fetch = fetch_body(&mut session, mailbox, uid, max_bytes).await?;
            let body = fetch.body().ok_or(MailError::NotFound)?;
            parsing::parse_message(body, &fetch)
        }
        .await
    })
}

/// The bytes of attachment `part_index` of message `uid`.
pub async fn fetch_attachment(
    config: &ServerConfig,
    mailbox: &str,
    uid: u32,
    part_index: u32,
    max_bytes: usize,
) -> Result<Vec<u8>, MailError> {
    check_mailbox(mailbox)?;
    with_session!(config, |session| {
        async {
            let fetch = fetch_body(&mut session, mailbox, uid, max_bytes).await?;
            let body = fetch.body().ok_or(MailError::NotFound)?;
            parsing::extract_attachment(body, part_index)
        }
        .await
    })
}

/// STORE `+FLAGS` or `-FLAGS` on a set of UIDs.
pub async fn set_flags(
    config: &ServerConfig,
    mailbox: &str,
    uids: &[u32],
    flags: &[String],
    add: bool,
) -> Result<(), MailError> {
    check_mailbox(mailbox)?;
    for flag in flags {
        check_flag(flag)?;
    }
    if uids.is_empty() || flags.is_empty() {
        return Ok(());
    }
    with_session!(config, |session| {
        async {
            session.select(mailbox).await.map_err(imap_err)?;
            let op = if add { "+FLAGS" } else { "-FLAGS" };
            let _: Vec<Fetch> = session
                .uid_store(uid_list(uids), format!("{op} ({})", flags.join(" ")))
                .await
                .map_err(imap_err)?
                .try_collect()
                .await
                .map_err(imap_err)?;
            Ok(())
        }
        .await
    })
}

/// MOVE, or COPY + `\Deleted` + UID EXPUNGE where the server has no MOVE.
pub async fn move_messages(
    config: &ServerConfig,
    source: &str,
    destination: &str,
    uids: &[u32],
) -> Result<(), MailError> {
    check_mailbox(source)?;
    check_mailbox(destination)?;
    if uids.is_empty() {
        return Ok(());
    }
    let set = uid_list(uids);
    with_session!(config, |session| {
        async {
            let capabilities = session.capabilities().await.map_err(imap_err)?;
            session.select(source).await.map_err(imap_err)?;
            if capabilities.has_str("MOVE") {
                return session.uid_mv(&set, destination).await.map_err(imap_err);
            }
            // Without UID EXPUNGE (UIDPLUS) the copy could not be undone in the source: the
            // messages would end up in both mailboxes.
            if !capabilities.has_str("UIDPLUS") {
                return Err(MailError::Imap(
                    "the server can neither MOVE nor UID EXPUNGE".into(),
                ));
            }
            session
                .uid_copy(&set, destination)
                .await
                .map_err(imap_err)?;
            let _: Vec<Fetch> = session
                .uid_store(&set, "+FLAGS (\\Deleted)")
                .await
                .map_err(imap_err)?
                .try_collect()
                .await
                .map_err(imap_err)?;
            let _: Vec<u32> = session
                .uid_expunge(&set)
                .await
                .map_err(imap_err)?
                .try_collect()
                .await
                .map_err(imap_err)?;
            Ok(())
        }
        .await
    })
}

/// APPEND a message (e.g. a sent one into "Sent").
pub async fn append_message(
    config: &ServerConfig,
    mailbox: &str,
    rfc822: &[u8],
    flags: &[String],
) -> Result<(), MailError> {
    check_mailbox(mailbox)?;
    for flag in flags {
        check_flag(flag)?;
    }
    let flags = (!flags.is_empty()).then(|| format!("({})", flags.join(" ")));
    with_session!(config, |session| {
        session
            .append(mailbox, flags.as_deref(), None, rfc822)
            .await
            .map_err(imap_err)
    })
}

/// A header value of an ENVELOPE, with RFC 2047 words decoded.
fn decode_header(raw: &[u8]) -> String {
    let mut header = raw.to_vec();
    header.push(b'\n');
    match MessageStream::new(&header).parse_unstructured() {
        HeaderValue::Text(text) => text.into_owned(),
        _ => String::from_utf8_lossy(raw).into_owned(),
    }
}

/// Converts an IMAP envelope address into the SDK address shape.
fn address_of(a: &ImapAddress) -> Address {
    let text = |b: &Option<std::borrow::Cow<'_, [u8]>>| {
        b.as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default()
    };
    let (mailbox, host) = (text(&a.mailbox), text(&a.host));
    Address {
        name: a.name.as_ref().map(|n| decode_header(n)),
        email: if host.is_empty() {
            mailbox
        } else {
            format!("{mailbox}@{host}")
        },
    }
}

/// Whether a BODYSTRUCTURE has a part beside the text and HTML body.
fn has_attachments(structure: &BodyStructure<'_>) -> bool {
    match structure {
        BodyStructure::Basic { .. } | BodyStructure::Message { .. } => true,
        BodyStructure::Text { common, .. } => common
            .disposition
            .as_ref()
            .is_some_and(|d| d.ty.eq_ignore_ascii_case("attachment")),
        BodyStructure::Multipart { bodies, .. } => bodies.iter().any(has_attachments),
    }
}

/// Converts an IMAP fetch response into the SDK envelope shape.
fn envelope_of(fetch: &Fetch) -> MessageEnvelope {
    let envelope = fetch.envelope();
    let addresses = |list: Option<&Vec<ImapAddress<'_>>>| {
        list.map(|l| l.iter().map(address_of).collect())
            .unwrap_or_default()
    };
    let text =
        |b: Option<&std::borrow::Cow<'_, [u8]>>| b.map(|b| String::from_utf8_lossy(b).into_owned());
    MessageEnvelope {
        uid: fetch.uid.unwrap_or(0),
        flags: fetch.flags().map(|f| parsing::flag_name(&f)).collect(),
        internal_date: fetch.internal_date().map(|d| d.timestamp()),
        subject: envelope
            .and_then(|e| e.subject.as_ref())
            .map(|s| decode_header(s)),
        from: addresses(envelope.and_then(|e| e.from.as_ref())),
        to: addresses(envelope.and_then(|e| e.to.as_ref())),
        cc: addresses(envelope.and_then(|e| e.cc.as_ref())),
        message_id: text(envelope.and_then(|e| e.message_id.as_ref())).map(bare_id),
        in_reply_to: text(envelope.and_then(|e| e.in_reply_to.as_ref())).map(bare_id),
        references: parsing::references_header(fetch),
        size: fetch.size,
        has_attachments: fetch.bodystructure().is_some_and(has_attachments),
    }
}

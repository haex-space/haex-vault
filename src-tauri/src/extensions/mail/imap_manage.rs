//! IMAP operations that look for, create or remove: search a mailbox, create one, delete messages
//! for good (spec 019, `contracts/host-mail-commands.md`). Like `imap.rs`, each one logs in, does
//! its work and logs out.

use futures::TryStreamExt;

use async_imap::types::Fetch;

use super::connect::{imap_login, imap_logout};
use super::imap::{imap_err, uid_list, with_session};
use super::{
    check_criteria, check_mailbox, parse_date, MailError, SearchCriteria, SearchResult,
    ServerConfig,
};

/// The most UIDs one search answers with and one delete takes; the same bound as `fetchEnvelopes`.
pub const MAX_UIDS: usize = super::imap::MAX_ENVELOPES;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A quoted IMAP string. The caller has refused control characters, so nothing in it can end the
/// command or start another one.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `YYYY-MM-DD` as IMAP writes a search date (`1-Jan-2026`).
fn imap_date(date: &str) -> Result<String, MailError> {
    let (year, month, day) =
        parse_date(date).ok_or_else(|| MailError::Invalid("date must be YYYY-MM-DD".into()))?;
    Ok(format!("{day}-{}-{year}", MONTHS[month as usize - 1]))
}

/// The `UID SEARCH` arguments for criteria that passed `check_criteria`. Text that is not ASCII
/// travels as quoted UTF-8 under `CHARSET UTF-8`: async-imap sends the command as one line and has
/// no literal; a server that refuses it answers with an error, which the caller sees.
pub(super) fn search_query(criteria: &SearchCriteria) -> Result<String, MailError> {
    let texts = [
        ("TEXT", &criteria.text),
        ("FROM", &criteria.from),
        ("TO", &criteria.to),
        ("SUBJECT", &criteria.subject),
    ];
    let mut parts: Vec<String> = Vec::new();
    if texts
        .iter()
        .any(|(_, text)| text.as_deref().is_some_and(|t| !t.is_ascii()))
    {
        parts.push("CHARSET UTF-8".into());
    }
    for (key, text) in texts {
        if let Some(text) = text {
            parts.push(format!("{key} {}", quoted(text)));
        }
    }
    if let Some(since) = &criteria.since {
        parts.push(format!("SINCE {}", imap_date(since)?));
    }
    if let Some(before) = &criteria.before {
        parts.push(format!("BEFORE {}", imap_date(before)?));
    }
    match criteria.unseen {
        Some(true) => parts.push("UNSEEN".into()),
        Some(false) => parts.push("SEEN".into()),
        None => {}
    }
    match criteria.flagged {
        Some(true) => parts.push("FLAGGED".into()),
        Some(false) => parts.push("UNFLAGGED".into()),
        None => {}
    }
    Ok(parts.join(" "))
}

/// The newest `limit` hits of a search in `mailbox`, newest (highest UID) first, and the number of
/// all hits.
pub async fn search(
    config: &ServerConfig,
    mailbox: &str,
    criteria: &SearchCriteria,
    limit: usize,
) -> Result<SearchResult, MailError> {
    check_mailbox(mailbox)?;
    check_criteria(criteria)?;
    if limit == 0 || limit > MAX_UIDS {
        return Err(MailError::Invalid("limit not allowed".into()));
    }
    let query = search_query(criteria)?;
    with_session!(config, |session| {
        async {
            session.select(mailbox).await.map_err(imap_err)?;
            let found = session.uid_search(&query).await.map_err(imap_err)?;
            let mut uids: Vec<u32> = found.into_iter().collect();
            uids.sort_unstable_by(|a, b| b.cmp(a));
            let total = u32::try_from(uids.len()).unwrap_or(u32::MAX);
            uids.truncate(limit);
            Ok(SearchResult { uids, total })
        }
        .await
    })
}

/// Creates `mailbox`; `false` when it was already there. A refused CREATE counts as "there" only
/// when LIST then names exactly that mailbox, so a real refusal (name, rights) stays an error.
pub async fn create_mailbox(config: &ServerConfig, mailbox: &str) -> Result<bool, MailError> {
    check_mailbox(mailbox)?;
    if mailbox.contains(['*', '%']) {
        return Err(MailError::Invalid("mailbox name not allowed".into()));
    }
    with_session!(config, |session| {
        async {
            let Err(refused) = session.create(mailbox).await else {
                return Ok(true);
            };
            let names: Vec<async_imap::types::Name> = session
                .list(Some(""), Some(mailbox))
                .await
                .map_err(imap_err)?
                .try_collect()
                .await
                .map_err(imap_err)?;
            if names.iter().any(|name| name.name() == mailbox) {
                Ok(false)
            } else {
                Err(imap_err(refused))
            }
        }
        .await
    })
}

/// Removes the messages `uids` of `mailbox` for good: `\Deleted` and UID EXPUNGE of exactly those.
/// Without UIDPLUS an expunge could take other messages that carry `\Deleted`, so the server is
/// asked first and nothing is marked when it cannot.
pub async fn delete_messages(
    config: &ServerConfig,
    mailbox: &str,
    uids: &[u32],
) -> Result<(), MailError> {
    check_mailbox(mailbox)?;
    if uids.len() > MAX_UIDS {
        return Err(MailError::TooLarge);
    }
    if uids.is_empty() {
        return Ok(());
    }
    let set = uid_list(uids);
    with_session!(config, |session| {
        async {
            let capabilities = session.capabilities().await.map_err(imap_err)?;
            if !capabilities.has_str("UIDPLUS") {
                return Err(MailError::Imap("the server cannot UID EXPUNGE".into()));
            }
            session.select(mailbox).await.map_err(imap_err)?;
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

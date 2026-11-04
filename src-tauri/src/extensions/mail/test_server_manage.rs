//! The test server's search with criteria, UID EXPUNGE, DELETE and RENAME (spec 019): what
//! `extension_mail_search`, `extension_mail_delete_messages`, `extension_mail_delete_mailbox` and
//! `extension_mail_rename_mailbox` talk to. Every message was received on 2026-01-01.

use super::{uids_of, words, State, Stored};

const KEYS: [&str; 11] = [
    "CHARSET",
    "FROM",
    "TO",
    "SUBJECT",
    "TEXT",
    "SINCE",
    "BEFORE",
    "UNSEEN",
    "SEEN",
    "FLAGGED",
    "UNFLAGGED",
];

/// Whether the arguments of `UID SEARCH` are criteria (and not a set of UIDs or sequence numbers).
pub(super) fn is_criteria(args: &str) -> bool {
    let first = args.split(' ').next().unwrap_or("").to_ascii_uppercase();
    KEYS.contains(&first.as_str())
}

/// The text of the header `name` (`From`, `To`, `Subject`) of a message.
fn header(m: &Stored, name: &str) -> String {
    let prefix = format!("{name}: ");
    String::from_utf8_lossy(&m.raw)
        .lines()
        .find_map(|l| l.strip_prefix(&prefix).map(str::to_owned))
        .unwrap_or_default()
}

/// A search date as IMAP writes it (`1-Jan-2026`) as year, month, day.
fn date(text: &str) -> (u32, u32, u32) {
    let mut parts = text.split('-');
    let day = parts.next().and_then(|d| d.parse().ok()).unwrap_or(0);
    let month = parts.next().unwrap_or("");
    let year = parts.next().and_then(|y| y.parse().ok()).unwrap_or(0);
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == month)
    .map_or(0, |i| i as u32 + 1);
    (year, month, day)
}

/// The UIDs of the messages that meet every criterion.
pub(super) fn search(args: &str, messages: &[Stored]) -> Vec<u32> {
    let words = words(args);
    messages
        .iter()
        .filter(|m| {
            let mut i = 0;
            while i < words.len() {
                let key = words[i].to_ascii_uppercase();
                let value = words.get(i + 1).cloned().unwrap_or_default();
                let seen = m.flags.iter().any(|f| f == "\\Seen");
                let flagged = m.flags.iter().any(|f| f == "\\Flagged");
                let contains =
                    |haystack: &str| haystack.to_lowercase().contains(&value.to_lowercase());
                let (holds, takes) = match key.as_str() {
                    "CHARSET" => (true, 1),
                    "FROM" => (contains(&header(m, "From")), 1),
                    "TO" => (contains(&header(m, "To")), 1),
                    "SUBJECT" => (contains(&header(m, "Subject")), 1),
                    "TEXT" => (contains(&String::from_utf8_lossy(&m.raw)), 1),
                    "SINCE" => ((2026, 1, 1) >= date(&value), 1),
                    "BEFORE" => ((2026, 1, 1) < date(&value), 1),
                    "UNSEEN" => (!seen, 0),
                    "SEEN" => (seen, 0),
                    "FLAGGED" => (flagged, 0),
                    "UNFLAGGED" => (!flagged, 0),
                    _ => (false, 0),
                };
                if !holds {
                    return false;
                }
                i += 1 + takes;
            }
            true
        })
        .map(|m| m.uid)
        .collect()
}

/// UID EXPUNGE: removes the messages of `set` that carry `\Deleted` from `selected`, and answers
/// with one `EXPUNGE` per message, numbered as at the time it went (those before it are gone).
pub(super) fn expunge(state: &mut State, selected: &str, set: &str) -> Vec<u8> {
    let Some(list) = state.boxes.get_mut(selected) else {
        return Vec::new();
    };
    let uids = uids_of(set, list);
    let mut out = Vec::new();
    let mut kept = 0;
    list.retain(|m| {
        let gone = uids.contains(&m.uid) && m.flags.iter().any(|f| f == "\\Deleted");
        if gone {
            out.extend(format!("* {} EXPUNGE\r\n", kept + 1).into_bytes());
        } else {
            kept += 1;
        }
        !gone
    });
    out
}

/// DELETE `name` or RENAME `from` `to`: `None` when done, else the refusal after `NO`.
pub(super) fn delete_or_rename(state: &mut State, verb: &str, args: &str) -> Option<&'static str> {
    const MISSING: &str = "[NONEXISTENT] Mailbox does not exist";
    let w = words(args);
    let (Some(from), to) = (w.first(), w.get(1)) else {
        return Some("[CANNOT] Mailbox name missing");
    };
    if !state.boxes.contains_key(from) {
        return Some(MISSING);
    }
    match (verb, to) {
        ("DELETE", _) => {
            state.boxes.remove(from);
        }
        (_, None) => return Some("[CANNOT] New name missing"),
        (_, Some(to)) if state.boxes.contains_key(to) => {
            return Some("[ALREADYEXISTS] Mailbox already exists")
        }
        (_, Some(to)) => {
            let messages = state.boxes.remove(from).unwrap_or_default();
            state.boxes.insert(to.clone(), messages);
        }
    }
    None
}

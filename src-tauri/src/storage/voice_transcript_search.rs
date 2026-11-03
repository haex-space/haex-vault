//! Searching the transcripts of spoken conversations (spec 066 T127, FR-048,
//! contracts/tauri-commands.md `assistant_transcript_search`).
//!
//! There is no full-text index in the project (006 rules it out): entries are folded with the
//! shared [`fold_for_search`] and compared as substrings. Results come in the stable order
//! `(thread_id, seq, id)` of the table's index; the cursor carries the last tuple and the next
//! page starts strictly after it.

use ::uuid::Uuid;
use haex_crdt::rusqlite::params;
use serde::Serialize;

use crate::passwords::ids::fold_for_search;
use crate::storage::query::Query;
use crate::storage::uuid;

/// Entries per page when the window asks for no limit.
pub const DEFAULT_LIMIT: usize = 50;
/// The largest page the window may ask for.
pub const MAX_LIMIT: usize = 100;
/// Characters of context on each side of the match in a snippet.
const CONTEXT_CHARS: usize = 40;
/// Rows read per query while scanning.
const BATCH: usize = 500;

/// Where the last page ended: `(thread_id, seq, id)` of its last entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub thread_id: Uuid,
    pub seq: i64,
    pub id: Uuid,
}

impl Cursor {
    /// The opaque form handed to the window.
    pub fn encode(&self) -> String {
        format!("{}:{}:{}", self.thread_id, self.seq, self.id)
    }

    /// Reads a cursor from [`Cursor::encode`]; `None` for anything else.
    pub fn decode(text: &str) -> Option<Self> {
        let mut parts = text.split(':');
        let cursor = Self {
            thread_id: Uuid::parse_str(parts.next()?).ok()?,
            seq: parts.next()?.parse().ok()?,
            id: Uuid::parse_str(parts.next()?).ok()?,
        };
        parts.next().is_none().then_some(cursor)
    }
}

/// A checked request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    /// The folded query; empty means nothing is searched.
    folded: String,
    after: Option<Cursor>,
    limit: usize,
}

impl SearchRequest {
    /// Checks the window's input: `limit` from 1 to [`MAX_LIMIT`] (default [`DEFAULT_LIMIT`]),
    /// a cursor from an earlier page. The error names what is wrong.
    pub fn new(query: &str, cursor: Option<&str>, limit: Option<u32>) -> Result<Self, String> {
        let limit = match limit {
            None => DEFAULT_LIMIT,
            Some(n) if (1..=MAX_LIMIT as u32).contains(&n) => n as usize,
            Some(n) => return Err(format!("limit {n} is not between 1 and {MAX_LIMIT}")),
        };
        let after = match cursor {
            None => None,
            Some(text) => Some(Cursor::decode(text).ok_or("the cursor is not one of a page")?),
        };
        Ok(Self {
            folded: fold(query.trim()).0,
            after,
            limit,
        })
    }
}

/// One matching entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub thread_id: Uuid,
    pub seq: i64,
    /// The entry itself: after a sync two entries of a thread can share a `seq`.
    pub id: Uuid,
    /// The match with up to 40 characters on each side, "…" where the text goes on.
    pub snippet: String,
}

/// A page of hits; `next_cursor` is set when more entries may match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub entries: Vec<SearchHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// The entries whose text contains the query, at most `limit` of them, after the cursor.
///
/// Rows are read [`BATCH`] at a time in the order of the index, so the scan ends as soon as one
/// hit more than the page is found instead of stepping through the whole table.
pub fn search(q: &mut impl Query, request: &SearchRequest) -> haex_crdt::Result<SearchPage> {
    if request.folded.is_empty() {
        return Ok(SearchPage {
            entries: Vec::new(),
            next_cursor: None,
        });
    }
    let mut hits: Vec<(Cursor, String)> = Vec::new();
    let mut position = request.after;
    'scan: loop {
        let rows = read_batch(q, position)?;
        let exhausted = rows.len() < BATCH;
        position = rows.last().map(|(cursor, _)| *cursor).or(position);
        for (cursor, text) in rows {
            if let Some(snippet) = snippet(&text, &request.folded) {
                hits.push((cursor, snippet));
                // One more than the page tells whether a next page exists.
                if hits.len() > request.limit {
                    break 'scan;
                }
            }
        }
        if exhausted {
            break;
        }
    }
    let more = hits.len() > request.limit;
    hits.truncate(request.limit);
    let next_cursor = more
        .then(|| hits.last().map(|(cursor, _)| cursor.encode()))
        .flatten();
    Ok(SearchPage {
        entries: hits
            .into_iter()
            .map(|(cursor, snippet)| SearchHit {
                thread_id: cursor.thread_id,
                seq: cursor.seq,
                id: cursor.id,
                snippet,
            })
            .collect(),
        next_cursor,
    })
}

/// Up to [`BATCH`] rows strictly after `after`, in the order of the index `(thread_id, seq, id)`;
/// row values compare as tuples.
fn read_batch(
    q: &mut impl Query,
    after: Option<Cursor>,
) -> haex_crdt::Result<Vec<(Cursor, String)>> {
    let map = |row: &haex_crdt::rusqlite::Row<'_>| {
        Ok((
            Cursor {
                thread_id: uuid::from_row(row, 0)?,
                seq: row.get(1)?,
                id: uuid::from_row(row, 2)?,
            },
            row.get::<_, String>(3)?,
        ))
    };
    let batch = BATCH as i64;
    match after {
        None => q.query_map(
            "SELECT thread_id, seq, id, text FROM voice_transcript_entries \
             ORDER BY thread_id, seq, id LIMIT ?1",
            params![batch],
            map,
        ),
        Some(cursor) => q.query_map(
            "SELECT thread_id, seq, id, text FROM voice_transcript_entries \
             WHERE (thread_id, seq, id) > (?1, ?2, ?3) ORDER BY thread_id, seq, id LIMIT ?4",
            params![
                cursor.thread_id.to_string(),
                cursor.seq,
                cursor.id.to_string(),
                batch
            ],
            map,
        ),
    }
}

/// Folds `text` one character at a time, so every folded character points back to the character
/// it came from. Query and text are folded alike, so a match in the folded text is a match of the
/// shared folding.
fn fold(text: &str) -> (String, Vec<usize>) {
    let mut folded = String::new();
    let mut origin = Vec::new();
    let mut buffer = [0u8; 4];
    for (index, c) in text.chars().enumerate() {
        for f in fold_for_search(c.encode_utf8(&mut buffer)).chars() {
            folded.push(f);
            origin.push(index);
        }
    }
    (folded, origin)
}

/// The part of `text` around the first match of the folded query, or `None` without a match.
fn snippet(text: &str, folded_query: &str) -> Option<String> {
    let (folded, origin) = fold(text);
    let byte = folded.find(folded_query)?;
    let first = folded[..byte].chars().count();
    let last = first + folded_query.chars().count() - 1;
    let (start, end) = (origin[first], origin[last] + 1);
    let chars: Vec<char> = text.chars().collect();
    let from = start.saturating_sub(CONTEXT_CHARS);
    let to = (end + CONTEXT_CHARS).min(chars.len());
    let mut out = String::new();
    if from > 0 {
        out.push('…');
    }
    out.extend(chars[from..to].iter());
    if to < chars.len() {
        out.push('…');
    }
    Some(out)
}

#[cfg(test)]
#[path = "voice_transcript_search_tests.rs"]
mod tests;

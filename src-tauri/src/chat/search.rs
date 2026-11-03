//! The chat sidebar's message search (spec 076 US2, FR-011 to FR-014, research R3/R4): the text of
//! contact, group and notes messages and of the agent's user and assistant messages, matched
//! literally and ignoring case (Unicode), newest first. Window only: no catalog action and no MCP
//! route reaches it (research R5), so agents keep reading contact conversations only with their
//! own grant (072 FR-071).

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::chat::peer::store::ConversationKind;
use crate::error::Result;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::query::Query;

/// Hits the window shows at most (FR-011).
pub const MAX_HITS: usize = 50;
/// Characters (Unicode scalar values) a text needs before messages are searched (FR-011).
const MIN_CHARS: usize = 2;
/// Characters of context kept before and after the start of the match (R4).
const BEFORE: usize = 40;
const AFTER: usize = 80;

/// One message the search found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatSearchHit"
)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// The conversation, or for `agent` the session.
    pub thread_id: String,
    pub kind: ConversationKind,
    pub message_id: String,
    /// `sort_at` of a contact message, `created_at` of an agent message.
    #[ts(type = "number")]
    pub at: i64,
    /// The text around the match, on one line, with `…` where it was cut.
    pub snippet: String,
}

/// The hits of one search.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ChatSearchResult"
)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// Newest first, at most [`MAX_HITS`].
    pub hits: Vec<SearchHit>,
    /// More messages matched than are listed.
    pub more: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchArgs {
    pub text: String,
}

/// Searches the messages of the open vault.
#[tauri::command]
pub async fn chat_search(state: State<'_, AppState>, args: SearchArgs) -> Result<SearchResult> {
    let db = active_database(&state)?;
    db.read(move |r| Ok(search(r, &args.text)?)).await
}

/// The newest [`MAX_HITS`] messages whose text contains `text`, and whether there are more.
///
/// ponytail: one pass over every message row per family, filtered in Rust because SQLite's `LIKE`
/// folds ASCII case only (R3); after one hit more than shown the remaining rows are only stepped
/// over. Upgrade when a vault misses SC-003: an indexed, lowercased shadow column.
pub fn search(q: &mut impl Query, text: &str) -> Result<SearchResult> {
    let needle = fold(&one_line(text));
    if needle.len() < MIN_CHARS {
        return Ok(SearchResult::default());
    }
    let mut hits = scan(
        q,
        "SELECT m.id, m.thread_id, m.text, m.sort_at, t.kind \
         FROM chat_peer_messages m JOIN chat_threads t ON t.id = m.thread_id \
         WHERE t.state <> 'request' AND m.text IS NOT NULL \
         ORDER BY m.sort_at DESC, m.id DESC",
        &needle,
    )?;
    hits.extend(scan(
        q,
        "SELECT id, thread_id, content, created_at, 'agent' FROM chat_messages \
         WHERE role IN ('user', 'assistant') \
         ORDER BY created_at DESC, id DESC",
        &needle,
    )?);
    hits.sort_by(|a, b| {
        b.at.cmp(&a.at)
            .then_with(|| b.message_id.cmp(&a.message_id))
    });
    let more = hits.len() > MAX_HITS;
    hits.truncate(MAX_HITS);
    Ok(SearchResult { hits, more })
}

/// Reads `sql` (columns: id, thread id, text, time, kind; newest first) in one pass and keeps the
/// first hits up to one more than [`MAX_HITS`]. One statement, not pages with `OFFSET`: no index
/// orders the messages globally, so every page would read and sort all rows again.
fn scan(q: &mut impl Query, sql: &str, needle: &[char]) -> Result<Vec<SearchHit>> {
    let mut hits = Vec::new();
    // Each row maps to `()`, so the rows themselves are not collected.
    q.query_map(sql, &[], |row| {
        if hits.len() > MAX_HITS {
            return Ok(());
        }
        let body: String = row.get(2)?;
        let Some(snippet) = snippet(&body, needle) else {
            return Ok(());
        };
        hits.push(SearchHit {
            thread_id: row.get(1)?,
            kind: ConversationKind::parse(&row.get::<_, String>(4)?)?,
            message_id: row.get(0)?,
            at: row.get(3)?,
            snippet,
        });
        Ok(())
    })?;
    Ok(hits)
}

/// Whitespace runs read as one space, the way [`snippet`] reads a message, so a text typed with two
/// spaces still finds the words it was typed from.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` lowercased per character, the way [`snippet`] compares.
fn fold(text: &str) -> Vec<char> {
    text.chars().flat_map(char::to_lowercase).collect()
}

/// The text around the first match of `needle` on one line, or `None` without a match. Whitespace
/// runs read as one space; the cut keeps characters whole.
fn snippet(body: &str, needle: &[char]) -> Option<String> {
    let line: Vec<char> = one_line(body).chars().collect();
    // Each lowercased character remembers the original character it came from.
    let folded: Vec<(char, usize)> = line
        .iter()
        .enumerate()
        .flat_map(|(at, c)| c.to_lowercase().map(move |l| (l, at)))
        .collect();
    let start = folded
        .windows(needle.len())
        .find(|window| window.iter().map(|(c, _)| *c).eq(needle.iter().copied()))?
        .first()?
        .1;
    let from = start.saturating_sub(BEFORE);
    let to = (start + AFTER).min(line.len());
    let mut out = String::new();
    if from > 0 {
        out.push('…');
    }
    out.extend(&line[from..to]);
    if to < line.len() {
        out.push('…');
    }
    Some(out)
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

//! The table `haex_agent_memory_pages` (spec 097 data-model.md). Reads take `&mut impl Query`,
//! writes the CRDT transaction. Every write is held to the check and the limits before anything is
//! written, so a refused write leaves the transaction untouched. The table has no UNIQUE constraint,
//! so the title rule is checked here, then written (check-then-write).

use haex_crdt::rusqlite::{params, Row};
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::check::{check, holds_vault_secret, CheckError, Field};
use super::limits::{appended_fits, fits, room_for_page, LimitError};
use super::{title_key, Author};
use crate::storage::query::Query;

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

/// Why a write was refused, or failed.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("{} {}", .0.as_str(), .1)]
    Check(Field, CheckError),
    #[error(transparent)]
    Limit(LimitError),
    #[error("another page has this title")]
    TitleTaken,
    #[error("no page has this title or id")]
    NotFound,
    #[error("the page changed after the approval was requested")]
    PageChanged,
    #[error("a new page needs a description")]
    DescriptionNeeded,
    #[error(transparent)]
    Db(#[from] haex_crdt::Error),
}

impl MemoryError {
    /// The code of the contract, with the field for a refusal of a form (`too_long:title`), so the
    /// window can say which field.
    pub fn reason(&self) -> String {
        match self {
            Self::Check(field, error @ (CheckError::Empty | CheckError::LineBreak)) => {
                format!("{}:{}", error.code(), field.as_str())
            }
            Self::Check(_, error) => error.code().to_owned(),
            Self::Limit(LimitError::TooLong(field)) => format!("too_long:{}", field.as_str()),
            Self::Limit(error) => error.code().to_owned(),
            Self::TitleTaken => "title_taken".to_owned(),
            Self::NotFound => "not_found".to_owned(),
            Self::PageChanged => "page_changed".to_owned(),
            Self::DescriptionNeeded => "description_needed".to_owned(),
            Self::Db(_) => "db".to_owned(),
        }
    }
}

/// Splits a store result for a CRDT write: a database error fails the transaction, a refusal is
/// handed back to the caller (nothing was written).
pub fn settle<T>(result: Result<T, MemoryError>) -> haex_crdt::Result<Result<T, MemoryError>> {
    match result {
        Err(MemoryError::Db(error)) => Err(error),
        other => Ok(other),
    }
}

/// A page as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub id: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub author_kind: String,
    pub author_key: Option<String>,
    pub author_name: String,
    pub author_device: String,
    pub created_at: i64,
    pub changed_kind: Option<String>,
    pub changed_key: Option<String>,
    pub changed_name: Option<String>,
    pub changed_device: Option<String>,
    pub changed_at: Option<i64>,
}

impl Page {
    /// When the page was written or changed last; the order of the list.
    pub fn last_change(&self) -> i64 {
        self.changed_at
            .map_or(self.created_at, |at| at.max(self.created_at))
    }
}

/// Title, description and text of a write.
#[derive(Debug, Clone, Copy)]
pub struct NewPage<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub body: &'a str,
}

/// A title resolved to its page, and the second page of a doubled title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Titled {
    pub page: Page,
    pub also: Option<Page>,
}

const COLUMNS: &str = "id, title, description, body, author_kind, author_key, author_name, \
     author_device, created_at, changed_kind, changed_key, changed_name, changed_device, changed_at";

fn page_of(row: &Row<'_>) -> haex_crdt::rusqlite::Result<Page> {
    Ok(Page {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        body: row.get(3)?,
        author_kind: row.get(4)?,
        author_key: row.get(5)?,
        author_name: row.get(6)?,
        author_device: row.get(7)?,
        created_at: row.get(8)?,
        changed_kind: row.get(9)?,
        changed_key: row.get(10)?,
        changed_name: row.get(11)?,
        changed_device: row.get(12)?,
        changed_at: row.get(13)?,
    })
}

/// Every page, changed last first.
pub fn list(q: &mut impl Query) -> haex_crdt::Result<Vec<Page>> {
    q.query_map(
        &format!(
            "SELECT {COLUMNS} FROM haex_agent_memory_pages \
             ORDER BY MAX(created_at, COALESCE(changed_at, created_at)) DESC, id"
        ),
        params![],
        page_of,
    )
}

/// The page with `id`.
pub fn get(q: &mut impl Query, id: &str) -> haex_crdt::Result<Option<Page>> {
    q.query_row(
        &format!("SELECT {COLUMNS} FROM haex_agent_memory_pages WHERE id = ?1"),
        params![id],
        page_of,
    )
}

/// How many pages the memory has.
pub fn count(q: &mut impl Query) -> haex_crdt::Result<usize> {
    let count: Option<i64> = q.query_row(
        "SELECT COUNT(*) FROM haex_agent_memory_pages",
        params![],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(count.unwrap_or(0)).unwrap_or(0))
}

/// The page `title` resolves to: of two with that title, the one changed last.
pub fn by_title(q: &mut impl Query, title: &str) -> haex_crdt::Result<Option<Titled>> {
    let key = title_key(title);
    // SQLite's `lower` folds ASCII only, so titles are compared here.
    let mut found = list(q)?
        .into_iter()
        .filter(|page| title_key(&page.title) == key);
    Ok(found.next().map(|page| Titled {
        page,
        also: found.next(),
    }))
}

/// Whether a page other than `except` has `title`.
fn title_taken(q: &mut impl Query, title: &str, except: Option<&str>) -> haex_crdt::Result<bool> {
    let key = title_key(title);
    let titles: Vec<(String, String)> = q.query_map(
        "SELECT id, title FROM haex_agent_memory_pages",
        params![],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(titles
        .iter()
        .any(|(id, title)| Some(id.as_str()) != except && title_key(title) == key))
}

/// The passwords and one-time-password secrets of the user's password manager, which never go into
/// the memory (FR-022). Read for every write: the password manager may have changed since the last.
pub fn vault_secrets(q: &mut impl Query) -> haex_crdt::Result<Vec<String>> {
    let rows: Vec<(Option<String>, Option<String>)> = q.query_map(
        "SELECT password, otp_secret FROM haex_passwords_item_details",
        params![],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(rows
        .into_iter()
        .flat_map(|(password, otp)| [password, otp])
        .flatten()
        .collect())
}

/// `text` for `field` passes the check, holds no value of the password manager, and fits.
pub(crate) fn checked(
    field: Field,
    text: &str,
    stored: Option<&str>,
    secrets: &[String],
) -> Result<(), MemoryError> {
    check(field, text).map_err(|error| MemoryError::Check(field, error))?;
    if holds_vault_secret(text, secrets) {
        return Err(MemoryError::Check(field, CheckError::Secret));
    }
    fits(field, text, stored).map_err(MemoryError::Limit)
}

fn checked_page(
    q: &mut impl Query,
    new: &NewPage<'_>,
    stored: Option<&Page>,
) -> Result<(), MemoryError> {
    let secrets = vault_secrets(q)?;
    checked(
        Field::Title,
        new.title,
        stored.map(|p| p.title.as_str()),
        &secrets,
    )?;
    checked(
        Field::Description,
        new.description,
        stored.map(|p| p.description.as_str()),
        &secrets,
    )?;
    checked(
        Field::Body,
        new.body,
        stored.map(|p| p.body.as_str()),
        &secrets,
    )
}

/// Adds a page.
pub fn add_page(
    tx: &mut CrdtTransaction<'_>,
    author: &Author,
    now: i64,
    new: &NewPage<'_>,
) -> Result<Page, MemoryError> {
    checked_page(tx, new, None)?;
    if title_taken(tx, new.title, None)? {
        return Err(MemoryError::TitleTaken);
    }
    room_for_page(count(tx)?).map_err(MemoryError::Limit)?;
    let id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO haex_agent_memory_pages \
         (id, title, description, body, author_kind, author_key, author_name, author_device, \
          created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            new.title.trim(),
            new.description.trim(),
            new.body,
            author.kind.as_str(),
            author.key,
            author.name,
            author.device.to_string(),
            now,
        ],
    )?;
    get(tx, &id)?.ok_or(MemoryError::NotFound)
}

fn write_change(
    tx: &mut CrdtTransaction<'_>,
    author: &Author,
    now: i64,
    id: &str,
    new: &NewPage<'_>,
) -> Result<Page, MemoryError> {
    tx.execute(
        "UPDATE haex_agent_memory_pages SET title = ?2, description = ?3, body = ?4, \
         changed_kind = ?5, changed_key = ?6, changed_name = ?7, changed_device = ?8, \
         changed_at = ?9 WHERE id = ?1",
        params![
            id,
            new.title.trim(),
            new.description.trim(),
            new.body,
            author.kind.as_str(),
            author.key,
            author.name,
            author.device.to_string(),
            now,
        ],
    )?;
    get(tx, id)?.ok_or(MemoryError::NotFound)
}

/// Replaces title, description and text of the page `id`.
pub fn change_page(
    tx: &mut CrdtTransaction<'_>,
    author: &Author,
    now: i64,
    id: &str,
    new: &NewPage<'_>,
) -> Result<Page, MemoryError> {
    let stored = get(tx, id)?.ok_or(MemoryError::NotFound)?;
    checked_page(tx, new, Some(&stored))?;
    if title_key(new.title) != title_key(&stored.title) && title_taken(tx, new.title, Some(id))? {
        return Err(MemoryError::TitleTaken);
    }
    write_change(tx, author, now, id, new)
}

/// Adds `text` as a paragraph to the page `title` (`memory.add`), or creates the page when the
/// title has none; creating needs a `description`. Answers the page and whether it is new.
pub fn append(
    tx: &mut CrdtTransaction<'_>,
    author: &Author,
    now: i64,
    title: &str,
    text: &str,
    description: Option<&str>,
) -> Result<(Page, bool), MemoryError> {
    let Some(Titled { page, .. }) = by_title(tx, title)? else {
        let description = description
            .filter(|d| !d.trim().is_empty())
            .ok_or(MemoryError::DescriptionNeeded)?;
        let new = NewPage {
            title,
            description,
            body: text.trim(),
        };
        return Ok((add_page(tx, author, now, &new)?, true));
    };
    // The added text is checked on its own; the length is the whole page's, checked below.
    check(Field::Body, text).map_err(|error| MemoryError::Check(Field::Body, error))?;
    let secrets = vault_secrets(tx)?;
    if holds_vault_secret(text, &secrets) {
        return Err(MemoryError::Check(Field::Body, CheckError::Secret));
    }
    let body = if page.body.trim().is_empty() {
        text.trim().to_owned()
    } else {
        format!("{}\n\n{}", page.body.trim_end(), text.trim())
    };
    check(Field::Body, &body).map_err(|error| MemoryError::Check(Field::Body, error))?;
    if holds_vault_secret(&body, &secrets) {
        return Err(MemoryError::Check(Field::Body, CheckError::Secret));
    }
    appended_fits(&page.body, &body).map_err(MemoryError::Limit)?;
    let new = NewPage {
        title: &page.title,
        description: &page.description,
        body: &body,
    };
    Ok((write_change(tx, author, now, &page.id, &new)?, false))
}

/// Removes the page `id`; its row leaves a deletion marker. Answers the page as it was.
pub fn remove_page(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<Page, MemoryError> {
    let stored = get(tx, id)?.ok_or(MemoryError::NotFound)?;
    tx.execute(
        "DELETE FROM haex_agent_memory_pages WHERE id = ?1",
        params![id],
    )?;
    Ok(stored)
}

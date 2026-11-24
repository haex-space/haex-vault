//! Commands of "KI → Gedächtnis" (spec 097 contracts, "Tauri commands"). Only the window calls them,
//! the writes behind the guardrail actions `settings.agentMemory.*`; no agent reaches one. The user
//! is the author of every write here, held to the check and the limits and never asked (FR-025).
//! The window addresses pages by id, so it reaches both pages of a doubled title. A refused write
//! is `InvalidInput` with the code of the contract as its reason (`title_taken`, `too_long:title`).
//! The page views follow every change through `vault-data-changed`, which the database raises for
//! each commit, so no command announces one itself.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use ts_rs::TS;

use super::limits::MAX_PAGES;
use super::links::pointers;
use super::search::{search, SearchPage};
use super::store::{self, settle, MemoryError, NewPage, Page};
use super::{title_key, Author};
use crate::error::{HaexError, Result};
use crate::state::AppState;
use crate::storage::known_devices;
use crate::storage::query::Query;
use crate::storage::wm_session_commands::current_device_uuid;

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;

/// A page in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentMemoryListItem {
    pub id: String,
    pub title: String,
    pub description: String,
    /// `user`, `model`, or a value a newer build wrote.
    pub author_kind: String,
    /// The model's name when it wrote; empty for the user.
    pub author_name: String,
    /// The current alias of the device it was written on; `None` when unknown.
    pub device_name: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    pub changed_name: Option<String>,
    #[ts(type = "number | null")]
    pub changed_at: Option<i64>,
    /// Another page has the same title (a sync made it).
    pub same_title: bool,
}

/// The list of "Gedächtnis": the pages shown and how many the memory has of how many it may.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentMemoryList {
    pub pages: Vec<AgentMemoryListItem>,
    pub count: usize,
    pub limit: usize,
}

/// A pointer of a page: its title, and the id of the page it leads to, `None` when there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentMemoryPointer {
    pub title: String,
    pub id: Option<String>,
}

/// One page as "Gedächtnis" shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentMemoryPage {
    pub id: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub author_kind: String,
    pub author_name: String,
    pub device_name: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    pub changed_kind: Option<String>,
    pub changed_name: Option<String>,
    pub changed_device_name: Option<String>,
    #[ts(type = "number | null")]
    pub changed_at: Option<i64>,
    pub same_title: bool,
    pub points_to: Vec<AgentMemoryPointer>,
    pub pointed_from: Vec<AgentMemoryPointer>,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentMemoryListArgs {
    pub query: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentMemoryPageArgs {
    pub id: String,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentMemoryAddArgs {
    pub title: String,
    pub description: String,
    pub body: String,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentMemoryChangeArgs {
    pub id: String,
    pub title: String,
    pub description: String,
    pub body: String,
}

/// The aliases of the vault's devices by their vault device UUID.
fn device_names(q: &mut impl Query) -> haex_crdt::Result<HashMap<String, Option<String>>> {
    Ok(known_devices::list_devices(q)?
        .into_iter()
        .map(|device| (device.vault_device_uuid.to_string(), device.alias))
        .collect())
}

/// The title keys held by more than one page.
fn doubled(pages: &[Page]) -> HashSet<String> {
    let mut seen = HashSet::new();
    pages
        .iter()
        .map(|page| title_key(&page.title))
        .filter(|key| !seen.insert(key.clone()))
        .collect()
}

fn item(
    page: &Page,
    names: &HashMap<String, Option<String>>,
    doubled: &HashSet<String>,
) -> AgentMemoryListItem {
    AgentMemoryListItem {
        id: page.id.clone(),
        title: page.title.clone(),
        description: page.description.clone(),
        author_kind: page.author_kind.clone(),
        author_name: page.author_name.clone(),
        device_name: names.get(&page.author_device).cloned().flatten(),
        created_at: page.created_at,
        changed_name: page.changed_name.clone(),
        changed_at: page.changed_at,
        same_title: doubled.contains(&title_key(&page.title)),
    }
}

/// The list, or with `query` the pages that match it, best first.
pub fn list_view(q: &mut impl Query, query: Option<&str>) -> haex_crdt::Result<AgentMemoryList> {
    let pages = store::list(q)?;
    let names = device_names(q)?;
    let doubled = doubled(&pages);
    let shown: Vec<&Page> = match query.map(str::trim).filter(|query| !query.is_empty()) {
        None => pages.iter().collect(),
        Some(query) => {
            let searchable: Vec<SearchPage<'_>> = pages.iter().map(searchable).collect();
            let by_id: HashMap<&str, &Page> = pages.iter().map(|p| (p.id.as_str(), p)).collect();
            search(&searchable, query)
                .iter()
                .filter_map(|hit| by_id.get(hit.id.as_str()).copied())
                .collect()
        }
    };
    Ok(AgentMemoryList {
        pages: shown
            .iter()
            .map(|page| item(page, &names, &doubled))
            .collect(),
        count: pages.len(),
        limit: MAX_PAGES,
    })
}

fn searchable(page: &Page) -> SearchPage<'_> {
    SearchPage {
        id: &page.id,
        title: &page.title,
        description: &page.description,
        body: &page.body,
        changed_at: page.last_change(),
    }
}

/// The page `id` with its pointers both ways.
pub fn page_view(q: &mut impl Query, id: &str) -> haex_crdt::Result<Option<AgentMemoryPage>> {
    let pages = store::list(q)?;
    let Some(page) = pages.iter().find(|page| page.id == id) else {
        return Ok(None);
    };
    let names = device_names(q)?;
    let doubled = doubled(&pages);
    // A title leads to the page changed last, as `store::by_title` resolves it.
    let mut ids: HashMap<String, String> = HashMap::new();
    for p in &pages {
        ids.entry(title_key(&p.title))
            .or_insert_with(|| p.id.clone());
    }
    let pointer = |title: String| AgentMemoryPointer {
        id: ids.get(&title_key(&title)).cloned(),
        title,
    };
    let listed = item(page, &names, &doubled);
    Ok(Some(AgentMemoryPage {
        id: listed.id,
        title: listed.title,
        description: listed.description,
        body: page.body.clone(),
        author_kind: listed.author_kind,
        author_name: listed.author_name,
        device_name: listed.device_name,
        created_at: listed.created_at,
        changed_kind: page.changed_kind.clone(),
        changed_name: listed.changed_name,
        changed_device_name: page
            .changed_device
            .as_ref()
            .and_then(|device| names.get(device).cloned().flatten()),
        changed_at: listed.changed_at,
        same_title: listed.same_title,
        points_to: pointers(&page.body).into_iter().map(pointer).collect(),
        pointed_from: pages
            .iter()
            .filter(|source| {
                ids.get(&title_key(&page.title))
                    .is_some_and(|resolved| resolved == &page.id)
                    && ids
                        .get(&title_key(&source.title))
                        .is_some_and(|resolved| resolved == &source.id)
                    && source.id != page.id
                    && pointers(&source.body)
                        .iter()
                        .any(|target| title_key(target) == title_key(&page.title))
            })
            .map(|source| pointer(source.title.clone()))
            .collect(),
    }))
}

/// The written page as the window shows it.
fn shown(
    tx: &mut haex_crdt::CrdtTransaction<'_>,
    written: std::result::Result<Page, MemoryError>,
) -> haex_crdt::Result<std::result::Result<AgentMemoryPage, MemoryError>> {
    match settle(written)? {
        Ok(page) => Ok(page_view(tx, &page.id)?.ok_or(MemoryError::NotFound)),
        Err(error) => Ok(Err(error)),
    }
}

/// Adds a page; the inner part of [`agent_memory_add`].
pub fn add_in(
    tx: &mut haex_crdt::CrdtTransaction<'_>,
    author: &Author,
    now: i64,
    new: &NewPage<'_>,
) -> haex_crdt::Result<std::result::Result<AgentMemoryPage, MemoryError>> {
    let written = store::add_page(tx, author, now, new);
    shown(tx, written)
}

/// Changes a page; the inner part of [`agent_memory_change`].
pub fn change_in(
    tx: &mut haex_crdt::CrdtTransaction<'_>,
    author: &Author,
    now: i64,
    id: &str,
    new: &NewPage<'_>,
) -> haex_crdt::Result<std::result::Result<AgentMemoryPage, MemoryError>> {
    let written = store::change_page(tx, author, now, id, new);
    shown(tx, written)
}

fn refused(error: MemoryError) -> HaexError {
    match error {
        MemoryError::Db(error) => error.into(),
        other => HaexError::InvalidInput {
            reason: other.reason(),
        },
    }
}

fn now() -> i64 {
    crate::passwords::clock::unix_millis(std::time::SystemTime::now())
}

/// The list of "Gedächtnis", or the pages matching `query`.
#[tauri::command]
pub async fn agent_memory_list(
    state: State<'_, AppState>,
    args: AgentMemoryListArgs,
) -> Result<AgentMemoryList> {
    state
        .database()?
        .read(move |q| list_view(q, args.query.as_deref()))
        .await
}

/// One page with its pointers.
#[tauri::command]
pub async fn agent_memory_page(
    state: State<'_, AppState>,
    args: AgentMemoryPageArgs,
) -> Result<AgentMemoryPage> {
    let id = args.id;
    state
        .database()?
        .read(move |q| page_view(q, &id))
        .await?
        .ok_or_else(|| refused(MemoryError::NotFound))
}

/// Adds a page written by the user.
#[tauri::command]
pub async fn agent_memory_add<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    args: AgentMemoryAddArgs,
) -> Result<AgentMemoryPage> {
    let db = state.database()?;
    let author = Author::user(current_device_uuid(&app, &db)?);
    let now = now();
    db.write(move |tx| {
        let new = NewPage {
            title: &args.title,
            description: &args.description,
            body: &args.body,
        };
        add_in(tx, &author, now, &new)
    })
    .await?
    .map_err(refused)
}

/// Replaces title, description and text of a page; the user becomes the one who changed it last.
#[tauri::command]
pub async fn agent_memory_change<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    args: AgentMemoryChangeArgs,
) -> Result<AgentMemoryPage> {
    let db = state.database()?;
    let author = Author::user(current_device_uuid(&app, &db)?);
    let now = now();
    db.write(move |tx| {
        let new = NewPage {
            title: &args.title,
            description: &args.description,
            body: &args.body,
        };
        change_in(tx, &author, now, &args.id, &new)
    })
    .await?
    .map_err(refused)
}

/// Removes a page on every device.
#[tauri::command]
pub async fn agent_memory_remove(
    state: State<'_, AppState>,
    args: AgentMemoryPageArgs,
) -> Result<()> {
    let id = args.id;
    state
        .database()?
        .write(move |tx| settle(store::remove_page(tx, &id)))
        .await?
        .map(|_| ())
        .map_err(refused)
}

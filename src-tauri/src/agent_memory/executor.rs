//! The five actions of the memory for agents (spec 097 FR-012, FR-016, FR-023, FR-028, contracts
//! "Actions of agents"): `memory.search`, `memory.open`, `memory.add`, `memory.write`,
//! `memory.remove`, addressed by title. Whether a write runs, asks or is refused was decided before
//! the call reaches here (`super::gate`); the right decided whether the model was offered the action
//! at all. The author of a write is the model of the running turn. Until spec 097 PR C only the
//! built-in agent's calls are served.

use std::collections::HashMap;

use async_trait::async_trait;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::limits::{max_chars, MAX_PAGES};
use super::links::LinkMap;
use super::search::{search, SearchPage};
use super::store::{self, settle, MemoryError, NewPage, Page};
use super::toc::{iso_date, NOTICE};
use super::{Author, AuthorKind};
use crate::chat::tools::agent_task;
use crate::chat::tools::native_action::{
    builtin_only, NativeCall, NativeError, NativeExecutor, NativeReply,
};
use crate::error::HaexError;
use crate::state::AppState;
use crate::storage::known_devices;
use crate::storage::wm_session_commands::current_device_uuid;
use crate::vault_gate::VaultDb;

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;

pub const SEARCH: &str = "memory.search";
pub const OPEN: &str = "memory.open";
pub const ADD: &str = "memory.add";
pub const WRITE: &str = "memory.write";
pub const REMOVE: &str = "memory.remove";
/// Written out, so `scripts/check-agent-actions.ts` reads them against the catalog.
pub const MEMORY_ACTION_IDS: &[&str] = &[
    "memory.search",
    "memory.open",
    "memory.add",
    "memory.write",
    "memory.remove",
];

/// Raised after every write of an agent; the window shows it as a notice (FR-023).
pub const EVENT_MEMORY_WRITTEN: &str = "agent-memory-written";

/// What a write of an agent did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WrittenKind {
    Added,
    Changed,
    Removed,
}

/// The payload of [`EVENT_MEMORY_WRITTEN`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWritten {
    pub kind: WrittenKind,
    pub page_id: String,
    pub title: String,
    /// The added text, the new text, or the text that was removed.
    pub text: String,
    pub author_name: String,
}

/// What the executor needs of the app.
pub trait MemoryEnv: Send + Sync {
    fn database(&self) -> Result<VaultDb, HaexError>;
    /// This device in the open vault.
    fn device(&self) -> BoxFuture<'_, Result<Uuid, HaexError>>;
    /// Tells the window what an agent wrote.
    fn written(&self, notice: MemoryWritten);
}

pub struct MemoryActions<E> {
    env: E,
}

impl<E: MemoryEnv> MemoryActions<E> {
    pub fn new(env: E) -> Self {
        Self { env }
    }
}

#[derive(Deserialize)]
struct SearchInput {
    query: String,
}

#[derive(Deserialize)]
struct TitleInput {
    title: String,
}

#[derive(Deserialize)]
struct AddInput {
    title: String,
    text: String,
    description: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteInput {
    title: String,
    description: String,
    text: String,
    new_title: Option<String>,
}

fn parse<T: for<'de> Deserialize<'de>>(input: Value) -> Result<T, NativeError> {
    serde_json::from_value(input)
        .map_err(|error| NativeError::invalid_input(None, error.to_string()))
}

fn internal(error: impl std::fmt::Display) -> NativeError {
    NativeError::new("internal", error.to_string())
}

/// A refusal as the model reads it: the code of the contract and what to do (contracts "Errors").
fn refused(error: MemoryError) -> NativeError {
    use super::check::CheckError;
    use super::limits::LimitError;
    let (code, field, message) = match &error {
        MemoryError::NotFound => ("not_found", None, "No page has this title. Search for it with memory_search, or create it with memory_add.".to_owned()),
        MemoryError::PageChanged => ("page_changed", None, "The page changed while approval was pending. Nothing was written. Open the page again before retrying.".to_owned()),
        MemoryError::DescriptionNeeded => ("description_needed", Some("description"), "A new page needs a description: give one line that says what the page is about.".to_owned()),
        MemoryError::TitleTaken => ("title_taken", Some("title"), "Another page has this title. Add to that page with memory_add instead.".to_owned()),
        MemoryError::Limit(LimitError::MemoryFull) => ("memory_full", None, format!("The memory is full ({MAX_PAGES} pages). Merge pages with memory_write or remove pages with memory_remove first.")),
        MemoryError::Limit(LimitError::PageTooLong) => ("page_too_long", Some("text"), "The page would become too long. Tidy it with memory_write, or start a new page.".to_owned()),
        MemoryError::Limit(LimitError::TooLong(field)) => ("too_long", Some(field_name(*field)), format!("The {} is too long: at most {} characters.", field_name(*field), max_chars(*field))),
        MemoryError::Check(field, CheckError::Empty) => ("empty", Some(field_name(*field)), format!("The {} must not be empty.", field_name(*field))),
        MemoryError::Check(field, CheckError::LineBreak) => ("line_break", Some(field_name(*field)), format!("The {} must be one line.", field_name(*field))),
        MemoryError::Check(_, CheckError::InvisibleCharacters) => ("invisible_characters", None, "The text carries invisible characters. Nothing was stored.".to_owned()),
        MemoryError::Check(_, CheckError::Secret) => ("secret", None, "Passwords, keys and tokens are never stored in the memory. Nothing was stored.".to_owned()),
        MemoryError::Check(_, CheckError::RoleMarker) => ("role_marker", None, "The text carries the role markers of a model's prompt. Nothing was stored.".to_owned()),
        MemoryError::Db(error) => return internal(error),
    };
    NativeError {
        code: code.to_owned(),
        field: field.map(str::to_owned),
        message,
    }
}

/// The input's name of a field: the page's text is `text` to a model.
fn field_name(field: super::check::Field) -> &'static str {
    match field {
        super::check::Field::Title => "title",
        super::check::Field::Description => "description",
        super::check::Field::Body => "text",
    }
}

/// The model of the running turn as the author of a write.
fn author(device: Uuid) -> Author {
    let task = agent_task::current();
    Author {
        kind: AuthorKind::Model,
        key: task
            .as_ref()
            .and_then(|task| task.subject.as_ref())
            .map(|subject| subject.key()),
        name: task.map(|task| task.model).unwrap_or_default(),
        device,
    }
}

fn now() -> i64 {
    crate::passwords::clock::unix_millis(std::time::SystemTime::now())
}

/// Who wrote a page as the model reads it.
fn by(kind: &str, name: &str) -> String {
    if AuthorKind::parse(kind) == Some(AuthorKind::User) {
        "the user".to_owned()
    } else {
        name.to_owned()
    }
}

fn opened(
    page: &Page,
    also: Option<&Page>,
    pages: &[Page],
    devices: &HashMap<String, String>,
) -> Value {
    let links = LinkMap::build(pages.iter().map(|p| (p.title.as_str(), p.body.as_str())));
    let device = |id: &str| {
        devices
            .get(id)
            .cloned()
            .unwrap_or_else(|| "an unknown device".to_owned())
    };
    let mut shown = json!({
        "title": page.title,
        "description": page.description,
        "text": page.body,
        "by": by(&page.author_kind, &page.author_name),
        "device": device(&page.author_device),
        "at": iso_date(page.created_at),
    });
    if let Some(at) = page.changed_at {
        shown["changedBy"] = json!(by(
            page.changed_kind.as_deref().unwrap_or_default(),
            page.changed_name.as_deref().unwrap_or_default()
        ));
        shown["changedAt"] = json!(iso_date(at));
    }
    let points_to: Vec<Value> = links
        .points_to(&page.title)
        .into_iter()
        .map(|pointer| {
            if pointer.missing {
                json!({ "title": pointer.title, "missing": true })
            } else {
                json!(pointer.title)
            }
        })
        .collect();
    let mut result = json!({
        "notice": NOTICE,
        "page": shown,
        "pointsTo": points_to,
        "pointedFrom": links.pointed_from(&page.title),
    });
    if let Some(also) = also {
        result["alsoTitled"] = json!({
            "description": also.description,
            "by": by(&also.author_kind, &also.author_name),
            "at": iso_date(also.created_at),
        });
    }
    result
}

impl<E: MemoryEnv> MemoryActions<E> {
    async fn read(&self, action_id: &str, input: Value) -> Result<Value, NativeError> {
        let db = self.env.database().map_err(internal)?;
        match action_id {
            SEARCH => {
                let query = parse::<SearchInput>(input)?.query;
                let pages = db.read(|q| store::list(q)).await.map_err(internal)?;
                let searchable: Vec<SearchPage<'_>> = pages
                    .iter()
                    .map(|p| SearchPage {
                        id: &p.id,
                        title: &p.title,
                        description: &p.description,
                        body: &p.body,
                        changed_at: p.last_change(),
                    })
                    .collect();
                let by_id: HashMap<&str, &Page> =
                    pages.iter().map(|page| (page.id.as_str(), page)).collect();
                let results: Vec<Value> = search(&searchable, &query)
                    .into_iter()
                    .filter_map(|hit| {
                        let page = by_id.get(hit.id.as_str())?;
                        Some(json!({
                            "title": hit.title,
                            "description": hit.description,
                            "excerpt": hit.excerpt,
                            "by": by(&page.author_kind, &page.author_name),
                            "at": iso_date(page.created_at),
                        }))
                    })
                    .collect();
                Ok(json!({ "notice": NOTICE, "results": results }))
            }
            _ => {
                let title = parse::<TitleInput>(input)?.title;
                let (found, pages, devices) = db
                    .read(move |q| {
                        let found = store::by_title(q, &title)?;
                        let devices: HashMap<String, String> = known_devices::list_devices(q)?
                            .into_iter()
                            .filter_map(|d| Some((d.vault_device_uuid.to_string(), d.alias?)))
                            .collect();
                        Ok((found, store::list(q)?, devices))
                    })
                    .await
                    .map_err(internal)?;
                let found = found.ok_or_else(|| refused(MemoryError::NotFound))?;
                Ok(opened(&found.page, found.also.as_ref(), &pages, &devices))
            }
        }
    }

    async fn write(&self, action_id: &str, input: Value) -> Result<Value, NativeError> {
        let db = self.env.database().map_err(internal)?;
        let author = author(self.env.device().await.map_err(internal)?);
        let name = author.name.clone();
        let now = now();
        let approved = super::gate::APPROVED_PAGE.try_with(Clone::clone).ok();
        enum Edit {
            Add(AddInput),
            Write(WriteInput),
            Remove(TitleInput),
        }

        let (edit, kind, added_text) = match action_id {
            ADD => {
                let input = parse::<AddInput>(input)?;
                let text = input.text.clone();
                (Edit::Add(input), WrittenKind::Added, Some(text))
            }
            WRITE => (Edit::Write(parse(input)?), WrittenKind::Changed, None),
            _ => (Edit::Remove(parse(input)?), WrittenKind::Removed, None),
        };
        let (page, created, pages) = db
            .write(move |tx| {
                let target = |tx: &mut haex_crdt::CrdtTransaction<'_>, title: &str| {
                    let found = store::by_title(tx, title)?.map(|found| found.page);
                    if approved.as_ref().is_some_and(|page| page != &found) {
                        return Err(MemoryError::PageChanged);
                    }
                    Ok(found)
                };
                let change = match edit {
                    Edit::Add(input) => {
                        let approval_check = if approved.is_some() {
                            target(tx, &input.title).map(|_| ())
                        } else {
                            Ok(())
                        };
                        approval_check.and_then(|()| {
                            store::append(
                                tx,
                                &author,
                                now,
                                &input.title,
                                &input.text,
                                input.description.as_deref(),
                            )
                        })
                    }
                    Edit::Write(input) => target(tx, &input.title).and_then(|found| {
                        let found = found.ok_or(MemoryError::NotFound)?;
                        let new = NewPage {
                            title: input.new_title.as_deref().unwrap_or(&found.title),
                            description: &input.description,
                            body: &input.text,
                        };
                        store::change_page(tx, &author, now, &found.id, &new)
                            .map(|page| (page, false))
                    }),
                    Edit::Remove(input) => target(tx, &input.title).and_then(|found| {
                        let found = found.ok_or(MemoryError::NotFound)?;
                        store::remove_page(tx, &found.id).map(|page| (page, false))
                    }),
                };
                settle(change.and_then(|(page, created)| Ok((page, created, store::count(tx)?))))
            })
            .await
            .map_err(internal)?
            .map_err(refused)?;
        let mut result = match kind {
            WrittenKind::Added => json!({ "title": page.title, "created": created }),
            WrittenKind::Changed => json!({ "title": page.title }),
            WrittenKind::Removed => json!({}),
        };
        self.env.written(MemoryWritten {
            kind,
            page_id: page.id,
            title: page.title,
            text: added_text.unwrap_or(page.body),
            author_name: name,
        });
        result["pages"] = json!(pages);
        result["limit"] = json!(MAX_PAGES);
        Ok(result)
    }
}

/// The app as the executor's environment.
pub struct TauriMemoryEnv {
    app: AppHandle,
}

impl TauriMemoryEnv {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl MemoryEnv for TauriMemoryEnv {
    fn database(&self) -> Result<VaultDb, HaexError> {
        self.app.state::<AppState>().database()
    }

    fn device(&self) -> BoxFuture<'_, Result<Uuid, HaexError>> {
        Box::pin(async move {
            let db = self.database()?;
            current_device_uuid(&self.app, &db)
        })
    }

    fn written(&self, notice: MemoryWritten) {
        if let Err(error) = self.app.emit(EVENT_MEMORY_WRITTEN, notice) {
            log::warn!("agent memory: the notice did not reach the window: {error}");
        }
    }
}

#[async_trait]
impl<E: MemoryEnv + 'static> NativeExecutor for MemoryActions<E> {
    fn action_ids(&self) -> &'static [&'static str] {
        MEMORY_ACTION_IDS
    }

    async fn run(
        &self,
        action_id: &str,
        input: Value,
        call: &NativeCall,
        _cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        builtin_only(call)?;
        let result = match action_id {
            SEARCH | OPEN => self.read(action_id, input).await?,
            ADD | WRITE | REMOVE => self.write(action_id, input).await?,
            other => {
                return Err(NativeError::new(
                    "unknown_action",
                    format!("{other} is no action of the memory"),
                ))
            }
        };
        Ok(result.into())
    }
}

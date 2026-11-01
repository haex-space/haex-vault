//! The native executor of the `files.*` actions (spec 044 US6, contracts/agent-actions.md, ADR
//! 0011), for the built-in agent and for each external agent (spec 021 US5, [`super::agents`]).
//! Every call runs as the agent it was built for, never as a caller named in the input:
//! holzi's own places are refused and left out of every answer (FR-033), the device needs the grant
//! "files of the device" (FR-031), a storage its own grant, asked for once when it is missing
//! (FR-031a; [`super::reach`]). Reading is in [`super::read`], the writing actions are in
//! [`super::exec_write`].

use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::env::AgentEnv;
use super::folders::TaskMemory;
use super::reach::{blocking, cancelled, entry_json, files_error, parse, PathInput, Place};
use crate::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};
use crate::files::access::is_device_storage;
use crate::files::access::{storage_listed, visible_to, StorageGrant, Want};
use crate::files::kind::FileCategory;
use crate::files::local::drives::{display_name, drives};
use crate::files::local::ops;
use crate::files::permissions::BUILTIN_AGENT;
use crate::files::search::{
    search, search_storage, SearchEnd, SearchFilters, SearchHit, SearchLimits, SearchOptions,
};
use crate::files::storage_source::parse_iso_ms;
use crate::files::transfer::local::Removal;
use crate::files::transfer::platform_removal;
use crate::files::{EntryKind, SourceRef};
use crate::passwords::access::Caller;

/// The actions this executor runs; `files.show` opens a window and stays in the frontend.
pub const FILE_ACTION_IDS: &[&str] = &[
    "files.sources",
    "files.list",
    "files.stat",
    "files.search",
    "files.read",
    "files.folder.create",
    "files.copy",
    "files.rename",
    "files.move",
    "files.delete",
];

/// Entries of `files.list` unless the agent asks for fewer or more.
const LIST_LIMIT: usize = 500;
/// The most entries one `files.list` returns.
const LIST_LIMIT_MAX: usize = 2_000;

/// The file actions of one agent.
pub struct FilesAgent<E> {
    pub(super) env: E,
    pub(super) agent_id: String,
    pub(super) caller: Caller,
    /// How a delete on the device removes (FR-023).
    pub(super) removal: Removal,
    /// Answers and opaque names of the running task (spec 048 FR-031, FR-032).
    pub(super) memory: Mutex<TaskMemory>,
}

impl<E: AgentEnv> FilesAgent<E> {
    /// The executor of the agent of the holzi chat.
    pub fn builtin(env: E) -> Self {
        Self {
            env,
            agent_id: BUILTIN_AGENT.to_owned(),
            caller: Caller::BuiltinAgent,
            removal: platform_removal(),
            memory: Mutex::default(),
        }
    }

    /// The executor of the external agent `agent_id` (spec 021 US5): its grants are the rows of
    /// that id in `agent_file_permissions`, and the device needs its own grant (FR-031).
    pub fn external(env: E, agent_id: &str) -> Self {
        Self {
            env,
            agent_id: agent_id.to_owned(),
            caller: Caller::ExternalAgent {
                id: agent_id.to_owned(),
            },
            removal: platform_removal(),
            memory: Mutex::default(),
        }
    }
}

#[async_trait]
impl<E: AgentEnv + 'static> NativeExecutor for FilesAgent<E> {
    fn action_ids(&self) -> &'static [&'static str] {
        FILE_ACTION_IDS
    }

    async fn run(
        &self,
        action_id: &str,
        input: Value,
        call: &NativeCall,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        if call.caller != self.caller {
            return Err(NativeError::new(
                "forbidden_for_agents",
                "This action is not available to this agent.",
            ));
        }
        // An external agent has no task: an answer without "Erlaubnis merken" holds for this call.
        if self.caller != Caller::BuiltinAgent {
            self.forget_answers();
        }
        if action_id == "files.read" {
            return self.read(parse(input)?, cancel).await;
        }
        let result = match action_id {
            "files.sources" => self.sources().await,
            "files.list" => self.list(parse(input)?, cancel).await,
            "files.stat" => self.stat(parse(input)?, cancel).await,
            "files.search" => self.search(parse(input)?, cancel).await,
            "files.folder.create" => self.create_folder(parse(input)?, cancel).await,
            "files.rename" => self.rename(parse(input)?, cancel).await,
            "files.copy" => self.transfer(false, parse(input)?, cancel).await,
            "files.move" => self.transfer(true, parse(input)?, cancel).await,
            "files.delete" => self.delete(parse(input)?, cancel).await,
            other => Err(NativeError::new(
                "unknown_action",
                format!("{other} is no file action"),
            )),
        };
        result.map(NativeReply::from)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListInput {
    source: String,
    path: String,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchInput {
    source: String,
    path: String,
    query: String,
    types: Option<Vec<FileCategory>>,
    size_min: Option<u64>,
    size_max: Option<u64>,
    /// `YYYY-MM-DD` or an ISO time.
    modified_after: Option<String>,
    modified_before: Option<String>,
}

/// `2025-03-01` or `2025-03-01T10:00:00Z` in milliseconds.
fn date_ms(text: &str, field: &str) -> Result<i64, NativeError> {
    let full = if text.contains('T') {
        text.to_owned()
    } else {
        format!("{text}T00:00:00Z")
    };
    parse_iso_ms(&full)
        .ok_or_else(|| NativeError::invalid_input(Some(field), "use a date like 2025-03-01"))
}

impl<E: AgentEnv> FilesAgent<E> {
    /// Drives, known places and the storages the agent holds a grant for.
    async fn sources(&self) -> Result<Value, NativeError> {
        let grants = self.env.grants(&self.agent_id).await.map_err(files_error)?;
        let own = self.env.own();
        // Without the grant "files of the device" the device is not named at all.
        let device = visible_to(&self.caller, Path::new("/"), &own, &grants);
        let storages: Vec<Value> = self
            .env
            .storages()
            .await
            .map_err(files_error)?
            .into_iter()
            .filter(|storage| storage_listed(&self.caller, &storage.id, &grants))
            .map(|storage| {
                json!({
                    "source": format!("storage:{}", storage.id),
                    "name": storage.name,
                    "access": if grants.storages.get(&storage.id) == Some(&StorageGrant::ReadWrite)
                    { "readWrite" } else { "read" },
                })
            })
            .collect();
        let mut result = json!({ "storages": storages });
        if device {
            let drives = blocking(|| Ok(drives())).await?;
            result["device"] = json!({
                "source": "device",
                "drives": drives
                    .iter()
                    .map(|drive| json!({ "name": display_name(drive), "path": drive.path }))
                    .collect::<Vec<_>>(),
                "places": self
                    .env
                    .known()
                    .iter()
                    .filter(|place| !own.contains_resolved(Path::new(&place.path)))
                    .map(|place| json!({ "name": place.name, "path": place.path }))
                    .collect::<Vec<_>>(),
            });
        }
        Ok(result)
    }

    async fn list(
        &self,
        input: ListInput,
        cancel: &CancellationToken,
    ) -> Result<Value, NativeError> {
        let limit = input.limit.unwrap_or(LIST_LIMIT).clamp(1, LIST_LIMIT_MAX);
        let shown: Vec<Value> = match self.place_at(&input.source, &input.path).await? {
            Place::Device => {
                let real = self.device(&input.path, Want::Read, false).await?;
                let own = self.env.own();
                let grants = self.env.grants(&self.agent_id).await.map_err(files_error)?;
                let caller = self.caller.clone();
                let folder = real.clone();
                let listed = blocking(move || ops::list(&folder, &own)).await?;
                let mut shown: Vec<Value> = listed
                    .iter()
                    .filter(|entry| {
                        visible_to(&caller, Path::new(&entry.path), &self.env.own(), &grants)
                    })
                    .map(entry_json)
                    .collect();
                // Encrypted folders by name, or opaque until granted (spec 048 US7).
                shown.extend(self.device_folders(&real, listed).await?);
                shown
            }
            Place::Storage(storage) => {
                let (files, paths) = self
                    .storage_at(&storage, Want::Read, &[input.path], cancel)
                    .await?;
                let entries = files.list(&paths[0]).await.map_err(files_error)?;
                self.shown_entries(&storage, &files, &entries).await?
            }
        };
        let truncated = shown.len() > limit;
        Ok(json!({
            "entries": shown.into_iter().take(limit).collect::<Vec<_>>(),
            "truncated": truncated,
        }))
    }

    async fn stat(
        &self,
        input: PathInput,
        cancel: &CancellationToken,
    ) -> Result<Value, NativeError> {
        let (entry, files, _) = self.entry(&input, cancel).await?;
        match (files, self.place_at(&input.source, &input.path).await?) {
            (Some(files), Place::Storage(storage)) => Ok(self
                .shown_entries(&storage, &files, std::slice::from_ref(&entry))
                .await?
                .remove(0)),
            _ => Ok(entry_json(&entry)),
        }
    }

    async fn search(
        &self,
        input: SearchInput,
        cancel: &CancellationToken,
    ) -> Result<Value, NativeError> {
        let filters = SearchFilters {
            types: input.types,
            size_min: input.size_min,
            size_max: input.size_max,
            modified_from: input
                .modified_after
                .as_deref()
                .map(|text| date_ms(text, "modifiedAfter"))
                .transpose()?,
            modified_to: input
                .modified_before
                .as_deref()
                .map(|text| date_ms(text, "modifiedBefore"))
                .transpose()?,
        };
        let options = SearchOptions {
            filters,
            show_hidden: false,
            own: self.env.own(),
            hide_own: true,
            limits: SearchLimits::AGENT,
        };
        let mut hits: Vec<SearchHit> = Vec::new();
        let end = match self.place_at(&input.source, &input.path).await? {
            Place::Device => {
                let root = self.device(&input.path, Want::Read, false).await?;
                let cancel = cancel.clone();
                let query = input.query;
                let (end, found) = blocking(move || {
                    let mut found = Vec::new();
                    let end = search(&root, &query, &options, &cancel, &mut |batch| {
                        found.extend(batch)
                    });
                    Ok((end, found))
                })
                .await?;
                hits = found;
                end
            }
            Place::Storage(storage) => {
                let (files, paths) = self
                    .storage_at(&storage, Want::Read, &[input.path], cancel)
                    .await?;
                let end = search_storage(
                    &files,
                    &paths[0],
                    &input.query,
                    &options,
                    cancel,
                    &mut |batch| hits.extend(batch),
                    &mut |_| {},
                )
                .await;
                hits = self.shown_hits(&storage, &files, hits).await?;
                for hit in &mut hits {
                    hit.entry.path = self.named(&storage, &hit.entry.path);
                }
                end
            }
        };
        let truncated = match end {
            SearchEnd::Done { truncated } => truncated,
            SearchEnd::Cancelled => return Err(cancelled()),
        };
        hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
        Ok(json!({
            "hits": hits.iter().map(|hit| entry_json(&hit.entry)).collect::<Vec<_>>(),
            "truncated": truncated,
        }))
    }

    /// Where `files.show` opens (FR-032a), after the checks of `files.stat`: a folder itself, a file
    /// in its folder with its name.
    pub async fn show_target(
        &self,
        source: &str,
        path: &str,
        cancel: &CancellationToken,
    ) -> Result<(SourceRef, String, Option<String>), NativeError> {
        let input = PathInput {
            source: source.to_owned(),
            path: path.to_owned(),
        };
        let (source, on_device) = match self.place_at(source, path).await? {
            Place::Device => (SourceRef::Device, None),
            // The window names a folder on the device by its device path (spec 048 US7).
            Place::Storage(storage) if is_device_storage(&storage.id) => {
                (SourceRef::Device, Some(storage))
            }
            Place::Storage(storage) => (
                SourceRef::Storage {
                    storage_id: storage.id,
                },
                None,
            ),
        };
        let (entry, _, _) = self.entry(&input, cancel).await?;
        let at = match &on_device {
            Some(storage) => self.named(storage, &entry.path),
            None => entry.path.clone(),
        };
        if entry.kind == EntryKind::Dir {
            return Ok((source, at, None));
        }
        let folder = Path::new(&at).parent().map_or_else(
            || at.clone(),
            |folder| folder.to_string_lossy().into_owned(),
        );
        Ok((source, folder, Some(entry.name)))
    }
}

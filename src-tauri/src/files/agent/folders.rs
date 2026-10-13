//! Encrypted folders for agents (spec 048 US6, FR-030 to FR-034, contracts/agent-actions.md). Every
//! storage path of an agent goes through [`FilesAgent::storage_at`]: a path into an encrypted
//! folder needs a grant for exactly that folder ([`check_folder`]), asked for when it is missing.
//! The question names the folder, the model and whether it runs in the cloud; the answer holds for
//! the task and is stored only with "Erlaubnis merken". A folder the agent may not read shows as
//! "verschlüsselter Ordner <n>" with an opaque name that is valid only in its task; searches leave
//! it out, and a path that names it by its real name is not found.

use std::collections::HashMap;
use std::sync::{MutexGuard, PoisonError};

use data_encoding::BASE32_NOPAD;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::env::AgentEnv;
use super::exec::FilesAgent;
use super::prompt::{FilesAgentChoice, FilesAgentFolderQuestion, FilesAgentWant};
use super::reach::{entry_json, files_error};
use crate::chat::tools::agent_task;
use crate::chat::tools::native_action::NativeError;
use crate::files::access::{check_folder, Locality, StorageGrant, Verdict, Want};
use crate::files::browser_commands::StorageSource;
use crate::files::encrypted::device::{name_encrypted, DeviceRoot};
use crate::files::encrypted::format::encode_id;
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::state_error;
use crate::files::permissions::{folder_target, AgentFileReach, AgentFileStatus};
use crate::files::search::SearchHit;
use crate::files::storage_source::StorageFiles;
use crate::files::{EncryptedRole, EncryptedState, Entry, FilesError, FilesErrorCode};
use crate::sync::keys::random_bytes;

/// What the question says when no task names the model: one holzi does not know, so a cloud model
/// (FR-030a).
const UNKNOWN_MODEL: &str = "unknown";

/// What the agent was answered and shown in its current task: answers without "Erlaubnis merken"
/// and the opaque names of folders it may not see. A call of another task finds it empty, so
/// nothing outlives its turn; the process ends with the vault (spec 013), so nothing outlives a
/// lock either.
#[derive(Default)]
pub struct TaskMemory {
    task: Option<String>,
    /// By `<storage_id>/<folder_id>`.
    held: HashMap<String, StorageGrant>,
    /// Opaque name → storage id and the folder's path.
    refs: HashMap<String, (String, String)>,
}

/// An encrypted folder a path leads into.
struct Folder {
    /// `<storage_id>/<folder_id>`.
    target: String,
    name: String,
}

fn not_found() -> NativeError {
    files_error(FilesError::new(FilesErrorCode::NotFound, "not found"))
}

fn not_granted() -> NativeError {
    files_error(FilesError::new(
        FilesErrorCode::NotGranted,
        "this encrypted folder is not granted",
    ))
}

fn locality() -> Locality {
    agent_task::current().map_or(Locality::Cloud, |task| task.locality)
}

fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(at) => &path[..at],
    }
}

fn joined(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

impl<E: AgentEnv> FilesAgent<E> {
    /// The memory of the running task; another task's is dropped first.
    fn memory(&self) -> MutexGuard<'_, TaskMemory> {
        let mut memory = self.memory.lock().unwrap_or_else(PoisonError::into_inner);
        let task = agent_task::current().map(|task| task.id);
        if memory.task != task {
            *memory = TaskMemory {
                task,
                ..TaskMemory::default()
            };
        }
        memory
    }

    /// The opaque name of the folder at `path` of `storage_id`, the same within the task.
    fn opaque_name(&self, storage_id: &str, path: &str) -> String {
        let mut memory = self.memory();
        if let Some(name) = memory
            .refs
            .iter()
            .find(|(_, (storage, folder))| storage == storage_id && folder == path)
            .map(|(name, _)| name.clone())
        {
            return name;
        }
        let name = format!(
            "enc-{}",
            BASE32_NOPAD
                .encode(&*random_bytes::<10>())
                .to_ascii_lowercase()
        );
        memory
            .refs
            .insert(name.clone(), (storage_id.to_owned(), path.to_owned()));
        name
    }

    /// `path` with an opaque name of this task replaced by the folder's path, and whether it held
    /// one.
    pub(super) fn resolved(&self, storage_id: &str, path: &str) -> (String, bool) {
        let memory = self.memory();
        let parts: Vec<&str> = path.split('/').collect();
        for (at, part) in parts.iter().enumerate() {
            if let Some((storage, folder)) = memory.refs.get(*part) {
                if storage == storage_id {
                    let rest = parts[at + 1..].join("/");
                    let path = if rest.is_empty() {
                        folder.clone()
                    } else {
                        format!("{folder}/{rest}")
                    };
                    return (path, true);
                }
            }
        }
        (path.to_owned(), false)
    }

    /// The encrypted folder `path` leads into, if any; one that cannot be opened refuses.
    async fn folder_of(
        &self,
        storage: &StorageSource,
        files: &StorageFiles,
        path: &str,
    ) -> Result<Option<Folder>, FilesError> {
        let Route::Encrypted(at) = route(files, path).await? else {
            return Ok(None);
        };
        match (at.head.state, at.head.fid) {
            (EncryptedState::Readable, Some(fid)) => Ok(Some(Folder {
                target: folder_target(&storage.id, &encode_id(&fid)),
                name: at.head.display.clone(),
            })),
            (EncryptedState::Readable, None) => Err(state_error(EncryptedState::Damaged)),
            (state, _) => Err(state_error(state)),
        }
    }

    async fn folder_verdict(
        &self,
        storage: &StorageSource,
        folder: &str,
        want: Want,
        held: Option<StorageGrant>,
    ) -> Result<Verdict, NativeError> {
        let grants = self.env.grants(&self.agent_id).await.map_err(files_error)?;
        Ok(check_folder(
            &self.caller,
            &storage.id,
            folder,
            want,
            &grants,
            held,
            locality(),
        ))
    }

    fn held(&self, folder: &str) -> Option<StorageGrant> {
        self.memory().held.get(folder).copied()
    }

    /// Settles the encrypted folder `path` leads into: without a fitting grant the user is asked
    /// (one question at a time), but only for a folder the agent named by its opaque name; one
    /// named by its real name is not found unless it is granted already.
    async fn gate(
        &self,
        storage: &StorageSource,
        files: &StorageFiles,
        path: &str,
        by_ref: bool,
        want: Want,
        cancel: &CancellationToken,
    ) -> Result<(), NativeError> {
        let folder = match self.folder_of(storage, files, path).await {
            Ok(Some(folder)) => folder,
            Ok(None) => return Ok(()),
            Err(_) if !by_ref => return Err(not_found()),
            Err(error) => return Err(files_error(error)),
        };
        let mut held = self.held(&folder.target);
        let mut verdict = self
            .folder_verdict(storage, &folder.target, want, held)
            .await?;
        if verdict == Verdict::Ask && by_ref {
            let prompt = self.env.prompt();
            let _turn = prompt.turn().await;
            // The caller before may have asked the same.
            held = self.held(&folder.target);
            verdict = self
                .folder_verdict(storage, &folder.target, want, held)
                .await?;
            if verdict == Verdict::Ask {
                let grant = self.ask(storage, &folder, want, cancel).await?;
                verdict = self
                    .folder_verdict(storage, &folder.target, want, Some(grant))
                    .await?;
            }
        }
        // By its real name only an agent that may read the folder learns why it may not do more.
        let knows_name = by_ref
            || (want == Want::Write
                && self
                    .folder_verdict(storage, &folder.target, Want::Read, held)
                    .await?
                    == Verdict::Allowed);
        match verdict {
            Verdict::Allowed => Ok(()),
            _ if !knows_name => Err(not_found()),
            Verdict::Refused(error) => Err(files_error(error)),
            Verdict::Ask | Verdict::ReadOnly => Err(not_granted()),
        }
    }

    /// Asks the user for `folder`; the answer holds for the task and is stored with "Erlaubnis
    /// merken", for cloud models only with "auch für Cloud-Modelle" (FR-030a, FR-031).
    async fn ask(
        &self,
        storage: &StorageSource,
        folder: &Folder,
        want: Want,
        cancel: &CancellationToken,
    ) -> Result<StorageGrant, NativeError> {
        let task = agent_task::current();
        let (model, cloud) = task
            .as_ref()
            .map_or((UNKNOWN_MODEL.to_owned(), true), |task| {
                (task.model.clone(), task.locality == Locality::Cloud)
            });
        let wants = match want {
            Want::Read => FilesAgentWant::Read,
            Want::Write => FilesAgentWant::ReadWrite,
        };
        let question = FilesAgentFolderQuestion {
            name: folder.name.clone(),
            model,
            cloud,
        };
        let answer = self
            .env
            .prompt()
            .ask(
                &self.agent_id,
                &storage.id,
                &storage.name,
                wants,
                Some(question),
                cancel,
            )
            .await
            .ok_or_else(not_granted)?;
        let (grant, status) = match answer.choice {
            FilesAgentChoice::Read => (StorageGrant::Read, AgentFileStatus::Read),
            FilesAgentChoice::ReadWrite => (StorageGrant::ReadWrite, AgentFileStatus::ReadWrite),
            FilesAgentChoice::Deny => (StorageGrant::Denied, AgentFileStatus::Denied),
        };
        if task.is_some() {
            self.memory().held.insert(folder.target.clone(), grant);
        }
        if answer.remember {
            let reach = if answer.also_cloud {
                AgentFileReach::Cloud
            } else {
                AgentFileReach::Local
            };
            self.env
                .store_folder_grant(&self.agent_id, &folder.target, status, reach)
                .await
                .map_err(files_error)?;
        }
        Ok(grant)
    }

    /// The files of `storage` and `paths` with their opaque names resolved, once the agent may
    /// `want` the storage and every encrypted folder the paths lead into.
    pub(super) async fn storage_at(
        &self,
        storage: &StorageSource,
        want: Want,
        paths: &[String],
        cancel: &CancellationToken,
    ) -> Result<(StorageFiles, Vec<String>), NativeError> {
        let files = self.storage(storage, want, cancel).await?;
        let mut resolved = Vec::with_capacity(paths.len());
        for path in paths {
            let (path, by_ref) = self.resolved(&storage.id, path);
            self.gate(storage, &files, &path, by_ref, want, cancel)
                .await?;
            resolved.push(path);
        }
        Ok((files, resolved))
    }

    /// Whether the agent may read the encrypted folder at or above `path` without being asked.
    async fn may_read(
        &self,
        storage: &StorageSource,
        files: &StorageFiles,
        path: &str,
    ) -> Result<bool, NativeError> {
        let Ok(Some(folder)) = self.folder_of(storage, files, path).await else {
            return Ok(false);
        };
        let held = self.held(&folder.target);
        Ok(self
            .folder_verdict(storage, &folder.target, Want::Read, held)
            .await?
            == Verdict::Allowed)
    }

    /// `entries` as the agent sees them: an encrypted folder it may not read is
    /// "verschlüsselter Ordner <n>" (counted per answer) with its opaque name, nothing more.
    pub(super) async fn shown_entries(
        &self,
        storage: &StorageSource,
        files: &StorageFiles,
        entries: &[Entry],
    ) -> Result<Vec<Value>, NativeError> {
        let mut shown = Vec::with_capacity(entries.len());
        let mut hidden = 0;
        for entry in entries {
            let Some(info) = entry.encrypted else {
                shown.push(entry_json(entry));
                continue;
            };
            let visible = info.role == EncryptedRole::Inside
                || self.may_read(storage, files, &entry.path).await?;
            if visible {
                let mut value = entry_json(entry);
                value["encrypted"] = json!(true);
                shown.push(value);
                continue;
            }
            hidden += 1;
            let name = self.opaque_name(&storage.id, &entry.path);
            shown.push(json!({
                "name": format!("verschlüsselter Ordner {hidden}"),
                "path": joined(parent_of(&entry.path), &name),
                "kind": "dir",
                "encrypted": true,
                "ref": name,
            }));
        }
        Ok(shown)
    }

    /// The encrypted folders among `listed` (a listing of the ordinary device folder `folder`) as
    /// the agent sees them: by name once it may read one, else opaque (spec 048 US7). Their raw
    /// directories never show.
    pub(super) async fn device_folders(
        &self,
        folder: &std::path::Path,
        listed: Vec<Entry>,
    ) -> Result<Vec<Value>, NativeError> {
        let raw = |entry: &Entry| {
            entry.kind == crate::files::EntryKind::Dir
                && crate::files::encrypted::format::is_folder_prefix(&entry.name)
        };
        let Some(root) = DeviceRoot::of(folder).filter(DeviceRoot::same_paths) else {
            return Ok(Vec::new());
        };
        if !listed.iter().any(raw) {
            return Ok(Vec::new());
        }
        let files = self
            .env
            .open_storage(&root.storage_id)
            .await
            .map_err(files_error)?;
        let named = name_encrypted(&files, &root, folder, listed)
            .await
            .map_err(files_error)?;
        let encrypted: Vec<Entry> = named
            .into_iter()
            .filter(|entry| entry.encrypted.is_some())
            .collect();
        let storage = StorageSource {
            id: root.storage_id.clone(),
            name: super::reach::DEVICE_NAME.to_owned(),
        };
        self.shown_entries(&storage, &files, &encrypted).await
    }

    /// The hits of a storage search without anything from an encrypted folder the agent may not
    /// read (FR-032): not the folder, not its entries.
    pub(super) async fn shown_hits(
        &self,
        storage: &StorageSource,
        files: &StorageFiles,
        hits: Vec<SearchHit>,
    ) -> Result<Vec<SearchHit>, NativeError> {
        let mut shown = Vec::with_capacity(hits.len());
        for hit in hits {
            if hit.entry.encrypted.is_none()
                || self.may_read(storage, files, &hit.entry.path).await?
            {
                shown.push(hit);
            }
        }
        Ok(shown)
    }
}

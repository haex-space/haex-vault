//! How an agent reaches what a call names (spec 044 FR-031 to FR-033, SC-005): the source by id or
//! name, a device path after resolving `..` and links, a storage after its grant, asked for once
//! when it is missing; and the shapes in which refusals and entries reach the model.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::env::AgentEnv;
use super::exec::FilesAgent;
use super::prompt::{FilesAgentChoice, FilesAgentWant};
use crate::chat::tools::native_action::NativeError;
use crate::files::access::{check, StorageGrant, Target, Verdict, Want};
use crate::files::browser_commands::StorageSource;
use crate::files::encrypted::device::{has_raw_part, DeviceRoot};
use crate::files::encrypted::route::{route, Route};
use crate::files::local::{ops, resolve, resolve_entry};
use crate::files::permissions::AgentFileStatus;
use crate::files::storage_source::StorageFiles;
use crate::files::{Entry, EntryKind, FilesError, FilesErrorCode};

/// What the question about an encrypted folder on the device names as its storage.
pub(super) const DEVICE_NAME: &str = "Gerät";

/// Where a call looks.
pub(super) enum Place {
    Device,
    Storage(StorageSource),
}

/// The input of an action, or what is wrong with it.
pub(super) fn parse<T: DeserializeOwned>(input: Value) -> Result<T, NativeError> {
    serde_json::from_value(input)
        .map_err(|error| NativeError::invalid_input(None, error.to_string()))
}

/// A refusal of the file layer as the model sees it: `files_<code>` (`files_blocked`,
/// `files_not_granted`, …) and its message, which never holds contents or credentials.
pub(super) fn files_error(error: FilesError) -> NativeError {
    let code = serde_json::to_value(error.code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    let mut snake = String::from("files_");
    for c in code.chars() {
        if c.is_ascii_uppercase() {
            snake.push('_');
            snake.push(c.to_ascii_lowercase());
        } else {
            snake.push(c);
        }
    }
    NativeError::new(snake, error.message)
}

/// A storage the agent may not reach, or none of that name: the same answer, so an agent without
/// a grant does not learn that a storage exists (SC-005).
fn unavailable() -> NativeError {
    files_error(FilesError::new(
        FilesErrorCode::NotGranted,
        "no storage of that id or name is available",
    ))
}

pub(super) fn cancelled() -> NativeError {
    NativeError::new("tool_call_cancelled", "the call was cancelled")
}

/// Runs blocking file work off the async threads.
pub(super) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, FilesError> + Send + 'static,
) -> Result<T, NativeError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| NativeError::new("failed", "the action failed"))?
        .map_err(files_error)
}

/// An entry as the model reads it: no flags it cannot use, times as ISO dates.
pub(super) fn entry_json(entry: &Entry) -> Value {
    let mut value = json!({
        "name": entry.name,
        "path": entry.path,
        "kind": match entry.kind {
            EntryKind::File => "file",
            EntryKind::Dir => "dir",
        },
    });
    if let Some(size) = entry.size {
        value["size"] = json!(size);
    }
    if let Some(modified) = entry.modified_ms {
        value["modified"] = json!(crate::passwords::clock::format_millis(modified));
    }
    if entry.hidden {
        value["hidden"] = json!(true);
    }
    if entry.no_access {
        value["noAccess"] = json!(true);
    }
    value
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PathInput {
    pub(super) source: String,
    pub(super) path: String,
}

impl<E: AgentEnv> FilesAgent<E> {
    /// `"device"`, `"storage:<id>"` or `"storage:<name>"` (the name the user said).
    pub(super) async fn place(&self, source: &str) -> Result<Place, NativeError> {
        if source == "device" {
            return Ok(Place::Device);
        }
        let Some(wanted) = source.strip_prefix("storage:") else {
            return Err(NativeError::invalid_input(
                Some("source"),
                "use \"device\" or \"storage:<id or name>\"",
            ));
        };
        let storages = self.env.storages().await.map_err(files_error)?;
        if let Some(storage) = storages.iter().find(|s| s.id == wanted) {
            return Ok(Place::Storage(storage.clone()));
        }
        let named: Vec<&StorageSource> = storages.iter().filter(|s| s.name == wanted).collect();
        match named.as_slice() {
            [storage] => Ok(Place::Storage((*storage).clone())),
            [] => Err(unavailable()),
            _ => Err(NativeError::invalid_input(
                Some("source"),
                "several storages have that name",
            )),
        }
    }

    /// [`Self::place`] for `paths`: device paths into encrypted folders on the device (by their
    /// decrypted or opaque names) are the device as a storage, so the folder grants of spec 048
    /// apply (US7, FR-050). Such paths and ordinary device paths are never mixed in one call: the
    /// device's storage would pass the ordinary ones by holzi's checks of its own places.
    pub(super) async fn place_for(
        &self,
        source: &str,
        paths: &[String],
    ) -> Result<Place, NativeError> {
        let place = self.place(source).await?;
        if !matches!(place, Place::Device) {
            return Ok(place);
        }
        let mut device = None;
        let mut ordinary = false;
        for path in paths {
            match self.encrypted_on_device(path).await {
                Some(storage) => device = Some(storage),
                None => ordinary = true,
            }
        }
        match (device, ordinary) {
            (None, _) => Ok(Place::Device),
            (Some(storage), false) => Ok(Place::Storage(storage)),
            (Some(_), true) => Err(NativeError::invalid_input(
                Some("paths"),
                "paths in encrypted folders and ordinary paths go in separate calls",
            )),
        }
    }

    /// [`Self::place_for`] of one path.
    pub(super) async fn place_at(&self, source: &str, path: &str) -> Result<Place, NativeError> {
        self.place_for(source, std::slice::from_ref(&path.to_owned()))
            .await
    }

    /// The device as a storage when the device path `path` leads into an encrypted folder there.
    /// Only where its window paths are device paths (not yet on Windows).
    async fn encrypted_on_device(&self, path: &str) -> Option<StorageSource> {
        let device = Path::new(path);
        let root = DeviceRoot::of(device).filter(DeviceRoot::same_paths)?;
        let (resolved, by_ref) = self.resolved(&root.storage_id, path);
        if !by_ref && (has_raw_part(device) || device.symlink_metadata().is_ok()) {
            return None;
        }
        let files = self.env.open_storage(&root.storage_id).await.ok()?;
        match route(&files, &resolved).await {
            Ok(Route::Encrypted(_)) => Some(StorageSource {
                id: root.storage_id,
                name: DEVICE_NAME.to_owned(),
            }),
            _ => None,
        }
    }

    /// The resolved device path the agent may `want`; `entry` keeps a link itself.
    pub(super) async fn device(
        &self,
        path: &str,
        want: Want,
        entry: bool,
    ) -> Result<PathBuf, NativeError> {
        let resolver = if entry { resolve_entry } else { resolve };
        let real = resolver(Path::new(path)).map_err(files_error)?;
        let grants = self.env.grants(&self.agent_id).await.map_err(files_error)?;
        match check(
            &self.caller,
            &Target::Device(real.clone()),
            want,
            &self.env.own(),
            &grants,
        ) {
            Verdict::Allowed => Ok(real),
            Verdict::Refused(error) => Err(files_error(error)),
            Verdict::ReadOnly | Verdict::Ask => Err(files_error(FilesError::new(
                FilesErrorCode::NotGranted,
                "not granted",
            ))),
        }
    }

    /// The files of `storage` once the agent may `want` it; without a grant the user is asked
    /// (FR-031a), one question at a time.
    pub(super) async fn storage(
        &self,
        storage: &StorageSource,
        want: Want,
        cancel: &CancellationToken,
    ) -> Result<StorageFiles, NativeError> {
        if self.verdict(storage, want).await? == Verdict::Ask {
            let prompt = self.env.prompt();
            let _turn = prompt.turn().await;
            // The caller before may have asked the same.
            if self.verdict(storage, want).await? == Verdict::Ask {
                let wants = match want {
                    Want::Read => FilesAgentWant::Read,
                    Want::Write => FilesAgentWant::ReadWrite,
                };
                let answer = prompt
                    .ask(
                        &self.agent_id,
                        &storage.id,
                        &storage.name,
                        wants,
                        None,
                        cancel,
                    )
                    .await;
                let Some(answer) = answer else {
                    return Err(unavailable());
                };
                let status = match answer.choice {
                    FilesAgentChoice::Read => AgentFileStatus::Read,
                    FilesAgentChoice::ReadWrite => AgentFileStatus::ReadWrite,
                    FilesAgentChoice::Deny => AgentFileStatus::Denied,
                };
                self.env
                    .store_grant(&self.agent_id, &storage.id, status)
                    .await
                    .map_err(files_error)?;
            }
        }
        let grants = self.env.grants(&self.agent_id).await.map_err(files_error)?;
        match check(
            &self.caller,
            &Target::Storage(storage.id.clone()),
            want,
            &self.env.own(),
            &grants,
        ) {
            Verdict::Allowed => self
                .env
                .open_storage(&storage.id)
                .await
                .map_err(files_error),
            // Granted for reading: the agent knows the storage already.
            Verdict::Refused(error)
                if grants.storages.get(&storage.id) == Some(&StorageGrant::Read) =>
            {
                Err(files_error(error))
            }
            _ => Err(unavailable()),
        }
    }

    async fn verdict(&self, storage: &StorageSource, want: Want) -> Result<Verdict, NativeError> {
        let grants = self.env.grants(&self.agent_id).await.map_err(files_error)?;
        Ok(check(
            &self.caller,
            &Target::Storage(storage.id.clone()),
            want,
            &self.env.own(),
            &grants,
        ))
    }

    /// The entry at `input`, with the storage it lies on.
    pub(super) async fn entry(
        &self,
        input: &PathInput,
        cancel: &CancellationToken,
    ) -> Result<(Entry, Option<StorageFiles>, Option<PathBuf>), NativeError> {
        match self.place_at(&input.source, &input.path).await? {
            Place::Device => {
                let real = self.device(&input.path, Want::Read, false).await?;
                let own = self.env.own();
                let at = real.clone();
                let entry = blocking(move || ops::stat(&at, &own)).await?;
                Ok((entry, None, Some(real)))
            }
            Place::Storage(storage) => {
                let (files, paths) = self
                    .storage_at(
                        &storage,
                        Want::Read,
                        std::slice::from_ref(&input.path),
                        cancel,
                    )
                    .await?;
                let entry = files.stat(&paths[0]).await.map_err(files_error)?;
                Ok((entry, Some(files), None))
            }
        }
    }
}

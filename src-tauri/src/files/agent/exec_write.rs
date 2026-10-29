//! The writing file actions of agents (contracts/agent-actions.md): create a folder, rename, copy,
//! move and delete. They run without a window: the decision on a taken name is in `onConflict`
//! (a copy never replaces), a delete goes to the trash on desktops. Folders that hold one of
//! holzi's own places are refused as a whole (FR-033).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::env::AgentEnv;
use super::exec::FilesAgent;
use super::reach::{blocking, cancelled, entry_json, files_error, Place};
use crate::chat::tools::native_action::NativeError;
use crate::files::access::Want;
use crate::files::local::drives::space;
use crate::files::local::{edit, OwnPlaces};
use crate::files::transfer::local::{self, prepare, touches_own, Hooks, Job, Outcome, Removal};
use crate::files::transfer::remote::{prepare_remote, run_remote, RemoteJob, Side, RETRIES};
use crate::files::transfer::{same_device, ConflictChoice, TransferOp, TransferProgress};
use crate::files::{FilesError, FilesErrorCode};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreateFolderInput {
    source: String,
    path: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RenameInput {
    source: String,
    path: String,
    new_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum OnConflict {
    Skip,
    KeepBoth,
    Replace,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TransferInput {
    source: String,
    paths: Vec<String>,
    /// The folder they go into.
    to: String,
    /// Where `to` lies; the source when absent.
    to_source: Option<String>,
    on_conflict: Option<OnConflict>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeleteInput {
    source: String,
    paths: Vec<String>,
}

/// How a transfer of an agent ended.
struct Report {
    outcome: Outcome,
    progress: Option<TransferProgress>,
    conflicts: u64,
}

/// Runs `work` with the hooks of an agent: every taken name gets `choice`, progress is kept for the
/// summary.
fn run_with(
    choice: ConflictChoice,
    removal: Removal,
    cancel: &CancellationToken,
    work: impl FnOnce(Hooks<'_>) -> Outcome,
) -> Report {
    let mut progress = None;
    let mut conflicts = 0;
    let outcome = work(Hooks {
        cancel,
        ask: &mut |_| {
            conflicts += 1;
            Some((choice, false))
        },
        progress: &mut |now| progress = Some(now),
        removal,
        rename: &|from, to| std::fs::rename(from, to),
    });
    Report {
        outcome,
        progress,
        conflicts,
    }
}

/// The summary for the model, or why the transfer stopped.
fn summary(report: Report, choice: ConflictChoice) -> Result<Value, NativeError> {
    let (done, total) = report
        .progress
        .map_or((0, 0), |p| (p.items_done, p.items_total));
    match report.outcome {
        Outcome::Done => {
            let conflicts = match choice {
                ConflictChoice::Skip => "skipped",
                ConflictChoice::KeepBoth => "keptBoth",
                ConflictChoice::Replace => "replaced",
            };
            let mut result = json!({ "items": total });
            result[conflicts] = json!(report.conflicts);
            Ok(result)
        }
        Outcome::Cancelled => Err(cancelled()),
        Outcome::Failed(error) => {
            let mut refused = files_error(error);
            refused.message = format!("{} (after {done} of {total} items)", refused.message);
            Err(refused)
        }
    }
}

fn blocked() -> NativeError {
    files_error(FilesError::new(
        FilesErrorCode::Blocked,
        "holzi's own data is not available to agents",
    ))
}

/// Refuses sources that hold one of holzi's own places: copying them would read it.
fn refuse_own_trees(own: &OwnPlaces, sources: &[PathBuf]) -> Result<(), NativeError> {
    if sources.iter().any(|source| own.touches(source, true)) {
        return Err(blocked());
    }
    Ok(())
}

fn free_space(path: &Path) -> Option<u64> {
    space(path).map(|(_, free)| free)
}

impl<E: AgentEnv> FilesAgent<E> {
    pub(super) async fn create_folder(
        &self,
        input: CreateFolderInput,
        cancel: &CancellationToken,
    ) -> Result<Value, NativeError> {
        let entry = match self.place_at(&input.source, &input.path).await? {
            Place::Device => {
                let parent = self.device(&input.path, Want::Write, false).await?;
                let own = self.env.own();
                blocking(move || edit::create_folder(&parent, &input.name, &own)).await?
            }
            Place::Storage(storage) => {
                let (files, paths) = self
                    .storage_at(&storage, Want::Write, &[input.path], cancel)
                    .await?;
                let entry = files
                    .create_folder(&paths[0], &input.name)
                    .await
                    .map_err(files_error)?;
                return Ok(self.storage_entry_json(&storage, &entry));
            }
        };
        Ok(entry_json(&entry))
    }

    pub(super) async fn rename(
        &self,
        input: RenameInput,
        cancel: &CancellationToken,
    ) -> Result<Value, NativeError> {
        let entry = match self.place_at(&input.source, &input.path).await? {
            Place::Device => {
                let real = self.device(&input.path, Want::Write, true).await?;
                let own = self.env.own();
                refuse_own_trees(&own, std::slice::from_ref(&real))?;
                blocking(move || edit::rename(&real, &input.new_name, &own)).await?
            }
            Place::Storage(storage) => {
                let (files, paths) = self
                    .storage_at(&storage, Want::Write, &[input.path], cancel)
                    .await?;
                let entry = files
                    .rename(&paths[0], &input.new_name)
                    .await
                    .map_err(files_error)?;
                return Ok(self.storage_entry_json(&storage, &entry));
            }
        };
        Ok(entry_json(&entry))
    }

    /// Copies (`moving` false) or moves `paths` into the folder `to`, on one source or across.
    pub(super) async fn transfer(
        &self,
        moving: bool,
        input: TransferInput,
        cancel: &CancellationToken,
    ) -> Result<Value, NativeError> {
        let choice = match (input.on_conflict.unwrap_or(OnConflict::Skip), moving) {
            (OnConflict::Replace, false) => {
                return Err(NativeError::invalid_input(
                    Some("onConflict"),
                    "a copy never replaces; use skip or keepBoth",
                ))
            }
            (OnConflict::Replace, true) => ConflictChoice::Replace,
            (OnConflict::KeepBoth, _) => ConflictChoice::KeepBoth,
            (OnConflict::Skip, _) => ConflictChoice::Skip,
        };
        if input.paths.is_empty() {
            return Err(NativeError::invalid_input(
                Some("paths"),
                "name at least one path",
            ));
        }
        let (op, want) = if moving {
            (TransferOp::Move, Want::Write)
        } else {
            (TransferOp::Copy, Want::Read)
        };
        let from = self.place_for(&input.source, &input.paths).await?;
        let to = self
            .place_at(
                input.to_source.as_deref().unwrap_or(&input.source),
                &input.to,
            )
            .await?;
        let own = self.env.own();
        let cancel = cancel.clone();
        let removal = self.removal;

        if let (Place::Device, Place::Device) = (&from, &to) {
            let mut sources = Vec::new();
            for path in &input.paths {
                sources.push(self.device(path, want, true).await?);
            }
            refuse_own_trees(&own, &sources)?;
            let target = self.device(&input.to, Want::Write, false).await?;
            let job = Job {
                op,
                sources,
                target: Some(target),
            };
            if touches_own(&job, &own) {
                return Err(blocked());
            }
            let report = blocking(move || {
                let totals = prepare(&job, free_space, same_device)?;
                Ok(run_with(choice, removal, &cancel, |hooks| {
                    local::run(&job, &totals, hooks)
                }))
            })
            .await?;
            return summary(report, choice);
        }

        let (from, sources) = match from {
            Place::Device => {
                let mut sources = Vec::new();
                for path in &input.paths {
                    sources.push(self.device(path, want, true).await?);
                }
                refuse_own_trees(&own, &sources)?;
                let sources = sources
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                (Side::Device, sources)
            }
            Place::Storage(storage) => {
                let (files, paths) = self
                    .storage_at(&storage, want, &input.paths, &cancel)
                    .await?;
                (Side::Storage(Arc::new(files)), paths)
            }
        };
        let (to, target) = match to {
            Place::Device => {
                let target = self.device(&input.to, Want::Write, false).await?;
                // A folder merging into one of the same name that holds an own place.
                let merges_into_own = input.paths.iter().any(|path| {
                    Path::new(path)
                        .file_name()
                        .is_some_and(|name| own.touches(&target.join(name), true))
                });
                if merges_into_own {
                    return Err(blocked());
                }
                (Side::Device, target.to_string_lossy().into_owned())
            }
            Place::Storage(storage) => {
                let (files, mut paths) = self
                    .storage_at(
                        &storage,
                        Want::Write,
                        std::slice::from_ref(&input.to),
                        &cancel,
                    )
                    .await?;
                (Side::Storage(Arc::new(files)), paths.remove(0))
            }
        };
        let job = RemoteJob {
            op,
            from,
            sources,
            to,
            target,
            known_free: false,
        };
        let report = blocking(move || {
            let handle = tokio::runtime::Handle::current();
            let plan = prepare_remote(&job, &handle, free_space)?;
            // Nobody can confirm for an agent that files leave an encrypted folder for an ordinary
            // one (spec 048 FR-019).
            if plan.leaving() > 0 {
                return Err(FilesError::new(
                    FilesErrorCode::NotGranted,
                    "files may not leave an encrypted folder for an ordinary folder",
                ));
            }
            Ok(run_with(choice, Removal::Permanent, &cancel, |hooks| {
                run_remote(&job, &plan, &handle, &RETRIES, hooks)
            }))
        })
        .await?;
        summary(report, choice)
    }

    /// Deletes `paths`: into the trash on desktops, for good on mobiles and on storages (FR-023).
    pub(super) async fn delete(
        &self,
        input: DeleteInput,
        cancel: &CancellationToken,
    ) -> Result<Value, NativeError> {
        if input.paths.is_empty() {
            return Err(NativeError::invalid_input(
                Some("paths"),
                "name at least one path",
            ));
        }
        match self.place_for(&input.source, &input.paths).await? {
            Place::Device => {
                let mut sources = Vec::new();
                for path in &input.paths {
                    sources.push(self.device(path, Want::Write, true).await?);
                }
                let own = self.env.own();
                refuse_own_trees(&own, &sources)?;
                let job = Job {
                    op: TransferOp::Delete,
                    sources,
                    target: None,
                };
                if touches_own(&job, &own) {
                    return Err(blocked());
                }
                let removal = self.removal;
                let cancel = cancel.clone();
                let count = job.sources.len();
                let report = blocking(move || {
                    let totals = prepare(&job, free_space, same_device)?;
                    Ok(run_with(ConflictChoice::Skip, removal, &cancel, |hooks| {
                        local::run(&job, &totals, hooks)
                    }))
                })
                .await?;
                summary(report, ConflictChoice::Skip)?;
                Ok(json!({ "deleted": count, "trash": removal == Removal::Trash }))
            }
            Place::Storage(storage) => {
                let (files, paths) = self
                    .storage_at(&storage, Want::Write, &input.paths, cancel)
                    .await?;
                for (done, path) in paths.iter().enumerate() {
                    if let Err(error) = files.delete(path, cancel, &mut |_| {}).await {
                        if cancel.is_cancelled() {
                            return Err(cancelled());
                        }
                        let mut refused = files_error(error);
                        refused.message = format!(
                            "{} (after {done} of {} entries)",
                            refused.message,
                            input.paths.len()
                        );
                        return Err(refused);
                    }
                }
                Ok(json!({ "deleted": input.paths.len(), "trash": false }))
            }
        }
    }
}

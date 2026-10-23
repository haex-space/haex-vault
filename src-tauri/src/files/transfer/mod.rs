//! Transfers of the file browser (spec 044 FR-018 to FR-026, data-model.md `Transfer`): copy, move
//! and delete run on the blocking pool as tracked work of the vault session, each with its own
//! token below the gate's, so closing the vault cancels them (FR-026, ADR 0003). The window follows
//! a transfer through a [`Channel`] and answers a name conflict with [`TransferManager::answer`].

pub(crate) mod entries;
pub mod local;
pub mod remote;
mod remote_plan;
mod remote_plan_encrypted;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::files::storage_source::StorageFiles;
use crate::files::{FilesError, FilesErrorCode};
use crate::vault_gate::VaultGate;
use local::{Hooks, Job, Outcome, Removal, Totals};
use remote::{Plan, RemoteJob};

/// What a transfer does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum TransferOp {
    Copy,
    Move,
    Delete,
}

/// The answer to a name that is taken at the target (FR-021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum ConflictChoice {
    Replace,
    KeepBoth,
    Skip,
}

/// The answer to files leaving an encrypted folder for an ordinary folder of a storage (spec 048
/// FR-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum LeaveChoice {
    Proceed,
    Cancel,
}

/// What the window answers a waiting transfer: a taken name or files leaving encryption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(untagged)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum TransferAnswer {
    Name(ConflictChoice),
    Leave(LeaveChoice),
}

/// How many files leave an encrypted folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct Leaving {
    #[ts(type = "number")]
    pub count: u64,
}

/// How far a transfer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct TransferProgress {
    #[ts(type = "number")]
    pub items_done: u64,
    #[ts(type = "number")]
    pub items_total: u64,
    #[ts(type = "number")]
    pub bytes_done: u64,
    #[ts(type = "number")]
    pub bytes_total: u64,
}

/// What the window hears of a transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum TransferEvent {
    Progress {
        progress: TransferProgress,
    },
    /// A name is taken; the transfer waits for [`TransferManager::answer`].
    Conflict {
        name: String,
    },
    /// Files of an encrypted folder go into an ordinary folder of a storage, where they lie
    /// unencrypted; before it starts the transfer waits for `proceed` or `cancel` (spec 048
    /// FR-019). The window sees it as a `conflict` with `leavesEncryption`.
    #[serde(rename = "conflict")]
    LeavesEncryption {
        #[serde(rename = "leavesEncryption")]
        leaves_encryption: Leaving,
    },
    Done,
    Cancelled,
    /// May start again with [`TransferManager::retry`].
    Failed {
        error: FilesError,
    },
}

/// How often progress goes to the window at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// How often a transfer waiting for an answer looks whether it was cancelled.
const ANSWER_POLL: Duration = Duration::from_millis(100);

/// What a transfer does.
#[derive(Clone)]
enum Work {
    /// Copy, move or delete on this device (blocking).
    Local {
        job: Job,
        totals: Totals,
        removal: Removal,
    },
    /// Delete entries of a storage for good (spec 044 US5, FR-023; asked for in the window).
    StorageDelete {
        files: Arc<StorageFiles>,
        paths: Vec<String>,
    },
    /// Copy or move between sources (spec 044 US5): up, down, within or between storages.
    Remote { job: RemoteJob, plan: Arc<Plan> },
}

/// A transfer the manager knows: running, or failed and able to start again.
struct Known {
    work: Work,
    channel: Channel<TransferEvent>,
    cancel: CancellationToken,
    answers: Sender<(TransferAnswer, bool)>,
}

/// The transfers of this session; a Tauri managed state.
#[derive(Clone, Default)]
pub struct TransferManager {
    known: Arc<Mutex<HashMap<String, Known>>>,
}

impl TransferManager {
    fn known(&self) -> MutexGuard<'_, HashMap<String, Known>> {
        self.known.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a prepared job on this device; returns its id.
    pub fn start(
        &self,
        gate: &VaultGate,
        job: Job,
        totals: Totals,
        removal: Removal,
        channel: Channel<TransferEvent>,
    ) -> Result<String, FilesError> {
        let id = uuid::Uuid::new_v4().to_string();
        let work = Work::Local {
            job,
            totals,
            removal,
        };
        self.launch(gate, &id, work, channel)?;
        Ok(id)
    }

    /// Deletes `paths` of a storage for good; returns the transfer's id.
    pub fn start_storage_delete(
        &self,
        gate: &VaultGate,
        files: StorageFiles,
        paths: Vec<String>,
        channel: Channel<TransferEvent>,
    ) -> Result<String, FilesError> {
        let id = uuid::Uuid::new_v4().to_string();
        let work = Work::StorageDelete {
            files: Arc::new(files),
            paths,
        };
        self.launch(gate, &id, work, channel)?;
        Ok(id)
    }

    /// Starts a prepared transfer across sources; returns its id.
    pub fn start_remote(
        &self,
        gate: &VaultGate,
        job: RemoteJob,
        plan: Plan,
        channel: Channel<TransferEvent>,
    ) -> Result<String, FilesError> {
        let id = uuid::Uuid::new_v4().to_string();
        let work = Work::Remote {
            job,
            plan: Arc::new(plan),
        };
        self.launch(gate, &id, work, channel)?;
        Ok(id)
    }

    fn launch(
        &self,
        gate: &VaultGate,
        id: &str,
        work: Work,
        channel: Channel<TransferEvent>,
    ) -> Result<(), FilesError> {
        let cancel = gate.token().child_token();
        let (answers, answered) = mpsc::channel();
        self.known().insert(
            id.to_owned(),
            Known {
                work: work.clone(),
                channel: channel.clone(),
                cancel: cancel.clone(),
                answers,
            },
        );
        let manager = self.clone();
        let owned_id = id.to_owned();
        let spawned = match work {
            Work::Local {
                job,
                totals,
                removal,
            } => gate
                .spawn_blocking(move || {
                    let outcome = work_local(&job, &totals, removal, &cancel, &channel, &answered);
                    manager.finish(&owned_id, outcome, &channel);
                })
                .map(drop),
            Work::Remote { job, plan } => gate
                .spawn_blocking(move || {
                    let handle = tokio::runtime::Handle::current();
                    let send = |event| {
                        let _ = channel.send(event);
                    };
                    let outcome = if confirm_leaving(&plan, &cancel, &send, &answered) {
                        with_hooks(Removal::Permanent, &cancel, &channel, &answered, |hooks| {
                            remote::run_remote(&job, &plan, &handle, &remote::RETRIES, hooks)
                        })
                    } else {
                        Outcome::Cancelled
                    };
                    manager.finish(&owned_id, outcome, &channel);
                })
                .map(drop),
            Work::StorageDelete { files, paths } => gate
                .spawn(async move {
                    let outcome = delete_on_storage(&files, &paths, &cancel, &channel).await;
                    manager.finish(&owned_id, outcome, &channel);
                })
                .map(drop),
        };
        if let Err(error) = spawned {
            self.known().remove(id);
            log::warn!("files: a transfer could not start: {error}");
            return Err(FilesError::new(
                FilesErrorCode::Unsupported,
                "the vault is closing",
            ));
        }
        Ok(())
    }

    /// Reports the end; a failed transfer stays known for [`Self::retry`].
    fn finish(&self, id: &str, outcome: Outcome, channel: &Channel<TransferEvent>) {
        let event = match outcome {
            Outcome::Done => TransferEvent::Done,
            Outcome::Cancelled => TransferEvent::Cancelled,
            Outcome::Failed(error) => TransferEvent::Failed { error },
        };
        if !matches!(event, TransferEvent::Failed { .. }) {
            self.known().remove(id);
        }
        let _ = channel.send(event);
    }

    /// Answers the question a transfer waits on.
    pub fn answer(&self, id: &str, choice: TransferAnswer, for_all: bool) {
        if let Some(known) = self.known().get(id) {
            let _ = known.answers.send((choice, for_all));
        }
    }

    /// Cancels a running transfer, or forgets a failed one.
    pub fn cancel(&self, id: &str) {
        if let Some(known) = self.known().remove(id) {
            known.cancel.cancel();
        }
    }

    /// Starts a failed transfer again with the same job and channel.
    pub fn retry(&self, gate: &VaultGate, id: &str) -> Result<(), FilesError> {
        let Some(known) = self.known().remove(id) else {
            return Err(FilesError::new(
                FilesErrorCode::NotFound,
                "no such transfer",
            ));
        };
        self.launch(gate, id, known.work, known.channel)
    }
}

/// Deletes `paths` of a storage one after the other; the progress counts the entries.
async fn delete_on_storage(
    files: &StorageFiles,
    paths: &[String],
    cancel: &CancellationToken,
    channel: &Channel<TransferEvent>,
) -> Outcome {
    let total = paths.len() as u64;
    for (done, path) in paths.iter().enumerate() {
        if cancel.is_cancelled() {
            return Outcome::Cancelled;
        }
        if let Err(error) = files.delete(path, cancel, &mut |_| {}).await {
            return if cancel.is_cancelled() {
                Outcome::Cancelled
            } else {
                Outcome::Failed(error)
            };
        }
        let _ = channel.send(TransferEvent::Progress {
            progress: TransferProgress {
                items_done: done as u64 + 1,
                items_total: total,
                bytes_done: 0,
                bytes_total: 0,
            },
        });
    }
    Outcome::Done
}

/// Asks the window once whether files may leave an encrypted folder for an ordinary folder of a
/// storage (spec 048 FR-019); true to go on. A retry after a yes does not ask again.
fn confirm_leaving(
    plan: &Plan,
    cancel: &CancellationToken,
    send: &dyn Fn(TransferEvent),
    answered: &Receiver<(TransferAnswer, bool)>,
) -> bool {
    let count = plan.leaving();
    if count == 0 {
        return true;
    }
    send(TransferEvent::LeavesEncryption {
        leaves_encryption: Leaving { count },
    });
    loop {
        match answered.recv_timeout(ANSWER_POLL) {
            Ok((TransferAnswer::Leave(choice), _)) => {
                let go = choice == LeaveChoice::Proceed;
                if go {
                    plan.confirm_leaving();
                }
                return go;
            }
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) if !cancel.is_cancelled() => {}
            Err(_) => return false,
        }
    }
}

/// The blocking part of a local job.
fn work_local(
    job: &Job,
    totals: &Totals,
    removal: Removal,
    cancel: &CancellationToken,
    channel: &Channel<TransferEvent>,
    answered: &Receiver<(TransferAnswer, bool)>,
) -> Outcome {
    with_hooks(removal, cancel, channel, answered, |hooks| {
        local::run(job, totals, hooks)
    })
}

/// Runs `work` with the hooks of a running transfer: progress at most every [`PROGRESS_EVERY`],
/// and the window asked about conflicts.
fn with_hooks(
    removal: Removal,
    cancel: &CancellationToken,
    channel: &Channel<TransferEvent>,
    answered: &Receiver<(TransferAnswer, bool)>,
    work: impl FnOnce(Hooks<'_>) -> Outcome,
) -> Outcome {
    let mut last = Instant::now() - PROGRESS_EVERY;
    let mut progress = |progress: TransferProgress| {
        let finished = progress.items_done == progress.items_total;
        if finished || last.elapsed() >= PROGRESS_EVERY {
            last = Instant::now();
            let _ = channel.send(TransferEvent::Progress { progress });
        }
    };
    let mut ask = |name: &str| {
        let _ = channel.send(TransferEvent::Conflict {
            name: name.to_owned(),
        });
        loop {
            match answered.recv_timeout(ANSWER_POLL) {
                Ok((TransferAnswer::Name(choice), for_all)) => return Some((choice, for_all)),
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) if !cancel.is_cancelled() => {}
                Err(_) => return None,
            }
        }
    };
    work(Hooks {
        cancel,
        ask: &mut ask,
        progress: &mut progress,
        removal,
        rename: &|from, to| std::fs::rename(from, to),
    })
}

/// How a delete removes on this platform: the trash on desktops, for good on mobiles (FR-023).
pub fn platform_removal() -> Removal {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        Removal::Permanent
    } else {
        Removal::Trash
    }
}

/// Whether two paths are on one file system (a move between them is a rename).
pub fn same_device(a: &std::path::Path, b: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::symlink_metadata(a), std::fs::metadata(b)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev(),
            _ => true,
        }
    }
    #[cfg(not(unix))]
    {
        a.components().next() == b.components().next()
    }
}

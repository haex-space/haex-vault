//! The narrow entry for sync rules (spec 061 research R2): one entry copied or moved between two
//! sides with the plan's answer to a taken name. Runs on a blocking thread like the rest of the
//! transfers.
//!
//! The plan says whether the target exists. When it does, the first question about the name answers
//! "Ersetzen" (the plan decided). Any other question means the target changed after the plan: a
//! file appeared where none was, or a peer's version check failed (spec 056 FR-036). That one
//! answers "Überspringen" and the transfer reports [`FilesErrorCode::Conflict`], so the next run
//! looks again instead of writing over a change nobody has seen.

use std::path::{Path, PathBuf};

use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

use super::local::{self, prepare, Hooks, Job, Outcome, Removal};
use super::remote::{prepare_remote, run_remote, RemoteJob, Side, RETRIES};
use super::{same_device, ConflictChoice, TransferOp, TransferProgress};
use crate::files::local::drives::space;
use crate::files::{FilesError, FilesErrorCode};

/// One entry to copy or move.
pub struct OneTransfer<'a> {
    pub op: TransferOp,
    pub from: &'a Side,
    /// A device path or a window path of the storage.
    pub path: &'a str,
    pub to: &'a Side,
    /// The folder it goes into, keeping its name.
    pub folder: &'a str,
    /// Whether the plan expects an entry of that name in `folder`.
    pub replaces: bool,
}

/// Runs `one`. Files leaving an encrypted folder for an ordinary folder of a storage are refused:
/// nobody confirms them while a rule runs (spec 048 FR-019).
pub fn transfer_one(
    one: &OneTransfer<'_>,
    handle: &Handle,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(TransferProgress),
) -> Outcome {
    let OneTransfer {
        op,
        from,
        path,
        to,
        folder,
        replaces,
    } = *one;
    let mut asked = 0u32;
    let mut skipped = false;
    let mut ask = |_: &str| {
        asked += 1;
        if replaces && asked == 1 {
            return Some((ConflictChoice::Replace, false));
        }
        skipped = true;
        Some((ConflictChoice::Skip, false))
    };
    let hooks = Hooks {
        cancel,
        ask: &mut ask,
        progress,
        removal: Removal::Permanent,
        rename: &|from, to| std::fs::rename(from, to),
    };
    let outcome = match (from, to) {
        (Side::Device, Side::Device) => {
            let job = Job {
                op,
                sources: vec![PathBuf::from(path)],
                target: Some(PathBuf::from(folder)),
            };
            match prepare(&job, free_space, same_device) {
                Ok(totals) => local::run(&job, &totals, hooks),
                Err(error) => Outcome::Failed(error),
            }
        }
        _ => {
            let job = RemoteJob {
                op,
                from: from.clone(),
                sources: vec![path.to_owned()],
                to: to.clone(),
                target: folder.to_owned(),
            };
            match prepare_remote(&job, handle, free_space) {
                Ok(plan) if plan.leaving() > 0 => Outcome::Failed(FilesError::new(
                    FilesErrorCode::NotGranted,
                    "files may not leave an encrypted folder for an ordinary folder",
                )),
                Ok(plan) => run_remote(&job, &plan, handle, &RETRIES, hooks),
                Err(error) => Outcome::Failed(error),
            }
        }
    };
    match outcome {
        Outcome::Done if skipped => Outcome::Failed(FilesError::new(
            FilesErrorCode::Conflict,
            "the target changed while the file went there",
        )),
        outcome => outcome,
    }
}

fn free_space(path: &Path) -> Option<u64> {
    space(path).map(|(_, free)| free)
}

#[cfg(test)]
#[path = "sync_copy_tests.rs"]
mod tests;

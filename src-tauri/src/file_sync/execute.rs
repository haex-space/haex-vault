//! Running a plan (research R2, R4): each step against the places, the state of a file written only
//! after its step succeeded (FR-015), a failing file noted and the rest going on (FR-023), and the
//! cancel token checked between steps and handed to every copy (FR-026).

use std::collections::{BTreeMap, BTreeSet};

use tokio_util::sync::CancellationToken;

use super::ignore::{union_text, IGNORE_FILE, MAX_BYTES};
use super::model::{SyncRule, SyncRuleSide};
use super::place::{split, Facts, Listing, Place, Trashed};
use super::plan::{Deviation, Step};
use super::progress::Reporter;
use super::run_log::{RunItem, SyncRuleRunAction};
use super::store::{self, PendingRow, Seen, StateRow};
use crate::files::transfer::local::Outcome;
use crate::files::transfer::remote::Side;
use crate::files::transfer::sync_copy::{transfer_one, OneTransfer};
use crate::files::transfer::TransferOp;
use crate::files::{FilesError, FilesErrorCode};
use crate::vault_gate::VaultDb;

/// The largest file of this device whose hash a copy notes for later moves (research R5).
const HASH_AFTER_COPY: u64 = 64 * 1024 * 1024;

/// What the steps of a run did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counts {
    pub copied: u64,
    pub moved: u64,
    pub deleted: u64,
    pub conflicts: u64,
    pub adopted: u64,
    pub kept: u64,
    pub pending: u64,
    pub deviations: u64,
    pub errors: u64,
}

/// A mirrored deletion and where it went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashedFile {
    pub side: SyncRuleSide,
    pub path: String,
    pub to: Trashed,
}

/// The most failed files a run keeps by name; [`Counts::errors`] counts all of them.
pub const MAX_FAILURES: usize = 200;

/// Failed files in a row after which a run stops: a storage that refuses every file would be asked
/// thousands of times, for every run again, and answers with a limit or a ban.
pub const FAILURES_IN_A_ROW: u32 = 10;

/// Why a run stopped before its plan was done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stopped {
    pub code: FilesErrorCode,
    pub reason: String,
}

/// Whether `code` will fail every other file the same way: the place does not answer, refuses the
/// credentials or its host key, or is full.
pub(super) fn stops_the_run(code: FilesErrorCode) -> bool {
    matches!(
        code,
        FilesErrorCode::Unreachable
            | FilesErrorCode::Credentials
            | FilesErrorCode::HostKey
            | FilesErrorCode::NoSpace
    )
}

/// What [`execute`] reports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Executed {
    pub counts: Counts,
    /// Files whose step failed, with the reason; the next run tries them again. Only the first
    /// [`MAX_FAILURES`] are named.
    pub failures: Vec<(String, String)>,
    /// Set when the run gave up before the end of its plan.
    pub stopped: Option<Stopped>,
    pub trashed: Vec<TrashedFile>,
    /// The conflict copies the run made (FR-012).
    pub conflict_copies: Vec<String>,
    /// Every file action, for the log (FR-043).
    pub items: Vec<RunItem>,
    pub cancelled: bool,
}

/// Where a run writes and what it runs against.
pub struct Target<'a> {
    pub db: &'a VaultDb,
    pub device: &'a str,
    pub rule: &'a SyncRule,
    pub run_id: &'a str,
    pub source: &'a dyn Place,
    pub target: &'a dyn Place,
    pub source_listing: &'a Listing,
    pub target_listing: &'a Listing,
    pub state: &'a [StateRow],
    pub cancel: &'a CancellationToken,
    pub progress: &'a Reporter,
}

impl Target<'_> {
    fn place(&self, side: SyncRuleSide) -> &dyn Place {
        match side {
            SyncRuleSide::Source => self.source,
            SyncRuleSide::Target => self.target,
        }
    }

    fn listing(&self, side: SyncRuleSide) -> &Listing {
        match side {
            SyncRuleSide::Source => self.source_listing,
            SyncRuleSide::Target => self.target_listing,
        }
    }
}

/// Why a step stopped.
enum StepError {
    Cancelled,
    Failed(FilesError),
}

impl From<FilesError> for StepError {
    fn from(error: FilesError) -> Self {
        Self::Failed(error)
    }
}

/// Runs `steps` in their order.
pub async fn execute(target: &Target<'_>, steps: &[Step]) -> Executed {
    let state: BTreeMap<&str, &StateRow> = target
        .state
        .iter()
        .map(|row| (row.path.as_str(), row))
        .collect();
    let mut done = Executed::default();
    let (files, bytes) = copy_totals(target, steps);
    target.progress.planned(files, bytes);
    let mut in_a_row = 0u32;
    let implied = implied_folders(target, steps);
    for step in steps {
        if target.cancel.is_cancelled() {
            done.cancelled = true;
            break;
        }
        if let Step::Folder { side, path } = step {
            if implied.contains(&(*side, path.clone())) {
                continue;
            }
        }
        match run_step(target, &state, step, &mut done).await {
            Ok(()) => in_a_row = 0,
            Err(StepError::Cancelled) => {
                done.cancelled = true;
                break;
            }
            Err(StepError::Failed(error)) => {
                done.counts.errors += 1;
                done.items.push(
                    RunItem::new(SyncRuleRunAction::Error, side_of(step), path_of(step))
                        .detail(error.message.clone()),
                );
                in_a_row += 1;
                let stop = stops_the_run(error.code) || in_a_row >= FAILURES_IN_A_ROW;
                if done.failures.len() < MAX_FAILURES {
                    done.failures
                        .push((path_of(step).to_owned(), error.message.clone()));
                }
                if stop {
                    log::warn!(
                        "file_sync: a run stops after {} failed files: {}",
                        done.counts.errors,
                        error.message
                    );
                    done.stopped = Some(Stopped {
                        code: error.code,
                        reason: error.message,
                    });
                    break;
                }
            }
        }
    }
    done
}

/// The folders a file of the plan goes into, on a place where that makes the folder: their steps
/// need no request (a folder of an object storage is the prefix of its objects).
fn implied_folders(target: &Target<'_>, steps: &[Step]) -> BTreeSet<(SyncRuleSide, String)> {
    let mut implied = BTreeSet::new();
    let mut add = |side: SyncRuleSide, path: &str| {
        if !target.place(side).implies_folders() {
            return;
        }
        let mut folder = split(path).0;
        while !folder.is_empty() {
            implied.insert((side, folder.to_owned()));
            folder = split(folder).0;
        }
    };
    for step in steps {
        match step {
            Step::Copy { from, path, .. } => add(from.other(), path),
            Step::Move { side, to, .. } => add(*side, to),
            _ => {}
        }
    }
    implied
}

/// How many files the copy steps move, and how many bytes they hold.
fn copy_totals(target: &Target<'_>, steps: &[Step]) -> (u32, u64) {
    steps
        .iter()
        .fold((0, 0), |(files, bytes), step| match step {
            Step::Copy { from, path, .. } => {
                let size = target
                    .listing(*from)
                    .files
                    .get(path.as_str())
                    .map_or(0, |facts| facts.size);
                (files + 1, bytes + size)
            }
            _ => (files, bytes),
        })
}

fn path_of(step: &Step) -> &str {
    match step {
        Step::Folder { path, .. }
        | Step::Aside { path, .. }
        | Step::Copy { path, .. }
        | Step::Conflict { path, .. }
        | Step::Adopt { path, .. }
        | Step::Delete { path, .. }
        | Step::Pending { path, .. }
        | Step::Keep { path }
        | Step::Report { path, .. }
        | Step::Forget { path } => path,
        Step::Move { from, .. } => from,
        Step::MergeIgnore { .. } => IGNORE_FILE,
    }
}

/// The side a step works on.
fn side_of(step: &Step) -> SyncRuleSide {
    match step {
        Step::Folder { side, .. }
        | Step::Move { side, .. }
        | Step::Aside { side, .. }
        | Step::Delete { side, .. }
        | Step::Pending { side, .. } => *side,
        Step::Copy { from, .. } => from.other(),
        Step::Conflict { keep, .. } => keep.other(),
        Step::Adopt { .. }
        | Step::Keep { .. }
        | Step::Report { .. }
        | Step::Forget { .. }
        | Step::MergeIgnore { .. } => SyncRuleSide::Target,
    }
}

fn seen(facts: &Facts) -> Seen {
    Seen {
        size: facts.size,
        mtime_ms: facts.mtime_ms,
    }
}

async fn run_step(
    target: &Target<'_>,
    state: &BTreeMap<&str, &StateRow>,
    step: &Step,
    done: &mut Executed,
) -> Result<(), StepError> {
    match step {
        Step::Folder { side, path } => {
            target.place(*side).create_folder(path).await?;
        }
        Step::Move { side, from, to } => {
            let place = target.place(*side);
            place.rename(from, to).await?;
            let moved = place.stat(to).await?.ok_or_else(changed_meanwhile)?;
            let source = target.listing(side.other()).files.get(to.as_str());
            let mut row = StateRow {
                path: to.clone(),
                source: None,
                target: None,
                kept: false,
                hash: state.get(from.as_str()).and_then(|row| row.hash.clone()),
            };
            set_side(&mut row, *side, Some(seen(&moved)));
            set_side(&mut row, side.other(), source.map(seen));
            write(target, move |tx, device, rule| {
                store::remove_state(tx, device, rule, &row.path)?;
                store::put_state(tx, device, rule, &row)
            })
            .await?;
            let gone = from.clone();
            write(target, move |tx, device, rule| {
                store::remove_state(tx, device, rule, &gone)
            })
            .await?;
            done.items.push(
                RunItem::new(SyncRuleRunAction::Move, *side, from.clone()).detail(to.clone()),
            );
            done.counts.moved += 1;
        }
        Step::Aside { side, path, to } => {
            target.place(*side).rename(path, to).await?;
            done.items.push(
                RunItem::new(SyncRuleRunAction::ConflictCopy, *side, path.clone())
                    .detail(to.clone()),
            );
            done.conflict_copies.push(to.clone());
            done.counts.conflicts += 1;
        }
        Step::Copy {
            from,
            path,
            replaces,
        } => {
            let planned = planned(target, *from, path)?;
            copy(target, *from, path, &planned, *replaces).await?;
            done.items.push(RunItem::new(
                SyncRuleRunAction::Copy,
                from.other(),
                path.clone(),
            ));
            done.counts.copied += 1;
        }
        Step::Conflict { path, keep, aside } => {
            let planned = planned(target, *keep, path)?;
            let older = keep.other();
            let set_aside = put_aside(target, older, path, aside, done).await?;
            copy(target, *keep, path, &planned, false).await?;
            done.items
                .push(RunItem::new(SyncRuleRunAction::Copy, older, path.clone()));
            done.counts.copied += 1;
            copy(target, older, aside, &set_aside, false).await?;
            done.items
                .push(RunItem::new(SyncRuleRunAction::Copy, *keep, aside.clone()));
            done.counts.copied += 1;
        }
        Step::MergeIgnore {
            aside_source,
            aside_target,
        } => {
            merge_ignore(target, aside_source, aside_target, done).await?;
        }
        Step::Adopt { path, hash } => {
            let (source, target_seen) = (
                target.source_listing.files.get(path.as_str()),
                target.target_listing.files.get(path.as_str()),
            );
            let row = StateRow {
                path: path.clone(),
                source: source.map(seen),
                target: target_seen.map(seen),
                kept: false,
                hash: Some(hash.clone()),
            };
            write(target, move |tx, device, rule| {
                store::put_state(tx, device, rule, &row)
            })
            .await?;
            done.items.push(RunItem::new(
                SyncRuleRunAction::Adopt,
                SyncRuleSide::Source,
                path.clone(),
            ));
            done.counts.adopted += 1;
        }
        Step::Delete { side, path } => {
            let to = target.place(*side).trash(path, target.run_id).await?;
            let gone = path.clone();
            write(target, move |tx, device, rule| {
                store::remove_state(tx, device, rule, &gone)
            })
            .await?;
            let mut item = RunItem::new(SyncRuleRunAction::Delete, *side, path.clone());
            item.trash = Some(to.clone());
            done.items.push(item);
            done.trashed.push(TrashedFile {
                side: *side,
                path: path.clone(),
                to,
            });
            done.counts.deleted += 1;
        }
        Step::Pending { side, path } => {
            let row = PendingRow {
                path: path.clone(),
                side: *side,
                found_run_id: target.run_id.to_owned(),
            };
            write(target, move |tx, device, rule| {
                store::put_pending(tx, device, rule, &row)
            })
            .await?;
            done.counts.pending += 1;
        }
        Step::Keep { path } => {
            if let Some(row) = state.get(path.as_str()) {
                let mut row = (*row).clone();
                row.kept = true;
                write(target, move |tx, device, rule| {
                    store::put_state(tx, device, rule, &row)
                })
                .await?;
            }
            done.counts.kept += 1;
        }
        Step::Report { path, what } => {
            // A file only at the target is noted, so it is reported once (FR-010).
            if *what == Deviation::Added {
                let row = StateRow {
                    path: path.clone(),
                    source: None,
                    target: target.target_listing.files.get(path.as_str()).map(seen),
                    kept: false,
                    hash: None,
                };
                write(target, move |tx, device, rule| {
                    store::put_state(tx, device, rule, &row)
                })
                .await?;
            }
            done.counts.deviations += 1;
        }
        Step::Forget { path } => {
            let gone = path.clone();
            write(target, move |tx, device, rule| {
                store::remove_state(tx, device, rule, &gone)
            })
            .await?;
        }
    }
    Ok(())
}

fn set_side(row: &mut StateRow, side: SyncRuleSide, value: Option<Seen>) {
    match side {
        SyncRuleSide::Source => row.source = value,
        SyncRuleSide::Target => row.target = value,
    }
}

fn changed_meanwhile() -> FilesError {
    FilesError::new(
        FilesErrorCode::Conflict,
        "the file changed while the run worked on it",
    )
}

/// The facts the plan saw of `path` on `side`.
fn planned(target: &Target<'_>, side: SyncRuleSide, path: &str) -> Result<Facts, FilesError> {
    target
        .listing(side)
        .files
        .get(path)
        .cloned()
        .ok_or_else(changed_meanwhile)
}

/// Moves the version of `path` on `side` to `aside` and returns its facts there.
async fn put_aside(
    target: &Target<'_>,
    side: SyncRuleSide,
    path: &str,
    aside: &str,
    done: &mut Executed,
) -> Result<Facts, FilesError> {
    let place = target.place(side);
    place.rename(path, aside).await?;
    done.conflict_copies.push(aside.to_owned());
    done.items
        .push(RunItem::new(SyncRuleRunAction::ConflictCopy, side, path).detail(aside.to_owned()));
    done.counts.conflicts += 1;
    place.stat(aside).await?.ok_or_else(changed_meanwhile)
}

/// Both exclusion files go aside and to the other side; the union of their patterns becomes the
/// exclusion file of both places (FR-039).
async fn merge_ignore(
    target: &Target<'_>,
    aside_source: &str,
    aside_target: &str,
    done: &mut Executed,
) -> Result<(), StepError> {
    let (source_place, target_place) = (
        target.place(SyncRuleSide::Source),
        target.place(SyncRuleSide::Target),
    );
    let text_source = source_place.read_text(IGNORE_FILE, MAX_BYTES).await?;
    let text_target = target_place.read_text(IGNORE_FILE, MAX_BYTES).await?;
    let (Some(text_source), Some(text_target)) = (text_source, text_target) else {
        return Err(changed_meanwhile().into());
    };
    let union = union_text(&text_source, &text_target);
    // Keep both originals live until their conflict copies exist on both places. If writing either
    // union fails, the next run can still read both exclusion sets from the original files.
    let set_source = preserve_ignore(
        target,
        SyncRuleSide::Source,
        &text_source,
        aside_source,
        done,
    )
    .await?;
    done.counts.conflicts += 1;
    let set_target = preserve_ignore(
        target,
        SyncRuleSide::Target,
        &text_target,
        aside_target,
        done,
    )
    .await?;
    copy(
        target,
        SyncRuleSide::Source,
        aside_source,
        &set_source,
        false,
    )
    .await?;
    done.items.push(RunItem::new(
        SyncRuleRunAction::Copy,
        SyncRuleSide::Target,
        aside_source,
    ));
    done.counts.copied += 1;
    copy(
        target,
        SyncRuleSide::Target,
        aside_target,
        &set_target,
        false,
    )
    .await?;
    done.items.push(RunItem::new(
        SyncRuleRunAction::Copy,
        SyncRuleSide::Source,
        aside_target,
    ));
    done.counts.copied += 1;
    source_place.write_text(IGNORE_FILE, &union).await?;
    done.items.push(RunItem::new(
        SyncRuleRunAction::Copy,
        SyncRuleSide::Source,
        IGNORE_FILE,
    ));
    target_place.write_text(IGNORE_FILE, &union).await?;
    done.items.push(RunItem::new(
        SyncRuleRunAction::Copy,
        SyncRuleSide::Target,
        IGNORE_FILE,
    ));
    let row = StateRow {
        path: IGNORE_FILE.to_owned(),
        source: source_place.stat(IGNORE_FILE).await?.as_ref().map(seen),
        target: target_place.stat(IGNORE_FILE).await?.as_ref().map(seen),
        kept: false,
        hash: None,
    };
    write(target, move |tx, device, rule| {
        store::put_state(tx, device, rule, &row)
    })
    .await?;
    Ok(())
}

async fn preserve_ignore(
    target: &Target<'_>,
    side: SyncRuleSide,
    text: &str,
    aside: &str,
    done: &mut Executed,
) -> Result<Facts, FilesError> {
    let place = target.place(side);
    if place.stat(aside).await?.is_some() {
        return Err(changed_meanwhile());
    }
    place.write_text(aside, text).await?;
    done.conflict_copies.push(aside.to_owned());
    done.items.push(
        RunItem::new(SyncRuleRunAction::ConflictCopy, side, IGNORE_FILE).detail(aside.to_owned()),
    );
    place.stat(aside).await?.ok_or_else(changed_meanwhile)
}

/// Copies `path` from `from` to the other side, checks that the source did not change since it
/// was `planned`, and notes both sides as they are now (FR-014 to FR-016).
async fn copy(
    target: &Target<'_>,
    from: SyncRuleSide,
    path: &str,
    planned: &Facts,
    replaces: bool,
) -> Result<(), StepError> {
    let result = copy_inner(target, from, path, planned, replaces).await;
    if result.is_err() {
        target.progress.copy_failed();
    }
    result
}

async fn copy_inner(
    target: &Target<'_>,
    from: SyncRuleSide,
    path: &str,
    planned: &Facts,
    replaces: bool,
) -> Result<(), StepError> {
    let (source, sink) = (target.place(from), target.place(from.other()));
    let (folder, _) = split(path);
    let one = Owned {
        from: source.side(),
        path: source.full_path(path),
        to: sink.side(),
        folder: sink.full_path(folder),
        replaces,
        // The plan listed the target for this run: a file it does not expect there is sent
        // without looking for it in a storage first; one it expects (`replaces`) is checked.
        known_free: !replaces,
        decrypts: target.rule.allow_decrypted,
    };
    let cancel = target.cancel.clone();
    let progress = target.progress.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let handle = tokio::runtime::Handle::current();
        transfer_one(&one.borrowed(), &handle, &cancel, &mut |done| {
            progress.bytes(done.bytes_done);
        })
    })
    .await
    .map_err(|_| FilesError::new(FilesErrorCode::Unsupported, "the copy stopped"))?;
    match outcome {
        Outcome::Done => {}
        Outcome::Cancelled => return Err(StepError::Cancelled),
        Outcome::Failed(error) => return Err(error.into()),
    }
    // A source that changed while it was copied is copied again by the next run.
    let now = source.stat(path).await?;
    if now.as_ref().map(seen) != Some(seen(planned)) {
        return Err(changed_meanwhile().into());
    }
    let copied = sink.stat(path).await?.ok_or_else(changed_meanwhile)?;
    if copied.size != planned.size {
        return Err(FilesError::new(
            FilesErrorCode::Broken,
            "the copy has another size than the file",
        )
        .into());
    }
    let hash = match planned.hash.clone() {
        Some(hash) => Some(hash),
        None if matches!(source.side(), Side::Device) && planned.size <= HASH_AFTER_COPY => {
            source.hash(path, target.cancel).await.ok()
        }
        None => None,
    };
    let mut row = StateRow {
        path: path.to_owned(),
        source: None,
        target: None,
        kept: false,
        hash,
    };
    set_side(&mut row, from, Some(seen(planned)));
    set_side(&mut row, from.other(), Some(seen(&copied)));
    write(target, move |tx, device, rule| {
        store::put_state(tx, device, rule, &row)
    })
    .await?;
    target.progress.file_done(planned.size);
    Ok(())
}

/// [`OneTransfer`] with owned parts, to move onto the blocking thread.
struct Owned {
    from: Side,
    path: String,
    to: Side,
    folder: String,
    replaces: bool,
    known_free: bool,
    decrypts: bool,
}

impl Owned {
    fn borrowed(&self) -> OneTransfer<'_> {
        OneTransfer {
            op: TransferOp::Copy,
            from: &self.from,
            path: &self.path,
            to: &self.to,
            folder: &self.folder,
            replaces: self.replaces,
            known_free: self.known_free,
            decrypts: self.decrypts,
        }
    }
}

/// Writes in one transaction for the rule of `target`.
async fn write(
    target: &Target<'_>,
    work: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>, &str, &str) -> crate::error::Result<()>
        + Send
        + 'static,
) -> Result<(), FilesError> {
    let (device, rule) = (target.device.to_owned(), target.rule.id.clone());
    target
        .db
        .write(move |tx| Ok(work(tx, &device, &rule)?))
        .await
        .map_err(|error| {
            log::warn!("file_sync: the state could not be written: {error}");
            FilesError::new(
                FilesErrorCode::Unsupported,
                "the state could not be written",
            )
        })
}

#[cfg(test)]
#[path = "execute_tests.rs"]
mod tests;

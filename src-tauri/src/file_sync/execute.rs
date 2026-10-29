//! Running a plan (research R2, R4): each step against the places, the state of a file written only
//! after its step succeeded (FR-015), a failing file noted and the rest going on (FR-023), and the
//! cancel token checked between steps and handed to every copy (FR-026).

use std::collections::BTreeMap;

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

/// What [`execute`] reports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Executed {
    pub counts: Counts,
    /// Files whose step failed, with the reason; the next run tries them again.
    pub failures: Vec<(String, String)>,
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
    pub a: &'a dyn Place,
    pub b: &'a dyn Place,
    pub listing_a: &'a Listing,
    pub listing_b: &'a Listing,
    pub state: &'a [StateRow],
    pub cancel: &'a CancellationToken,
    pub progress: &'a Reporter,
}

impl Target<'_> {
    fn place(&self, side: SyncRuleSide) -> &dyn Place {
        match side {
            SyncRuleSide::A => self.a,
            SyncRuleSide::B => self.b,
        }
    }

    fn listing(&self, side: SyncRuleSide) -> &Listing {
        match side {
            SyncRuleSide::A => self.listing_a,
            SyncRuleSide::B => self.listing_b,
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
    for step in steps {
        if target.cancel.is_cancelled() {
            done.cancelled = true;
            break;
        }
        match run_step(target, &state, step, &mut done).await {
            Ok(()) => {}
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
                done.failures
                    .push((path_of(step).to_owned(), error.message));
            }
        }
    }
    done
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
        | Step::MergeIgnore { .. } => SyncRuleSide::B,
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
                a: None,
                b: None,
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
            let set_aside = put_aside(target, older, path, aside).await?;
            copy(target, *keep, path, &planned, false).await?;
            copy(target, older, aside, &set_aside, false).await?;
            done.items.extend([
                RunItem::new(SyncRuleRunAction::ConflictCopy, older, path.clone())
                    .detail(aside.clone()),
                RunItem::new(SyncRuleRunAction::Copy, older, path.clone()),
                RunItem::new(SyncRuleRunAction::Copy, *keep, aside.clone()),
            ]);
            done.conflict_copies.push(aside.clone());
            done.counts.conflicts += 1;
            done.counts.copied += 2;
        }
        Step::MergeIgnore { aside_a, aside_b } => {
            merge_ignore(target, aside_a, aside_b).await?;
            for (side, aside) in [(SyncRuleSide::A, aside_a), (SyncRuleSide::B, aside_b)] {
                done.items.extend([
                    RunItem::new(SyncRuleRunAction::ConflictCopy, side, IGNORE_FILE)
                        .detail(aside.clone()),
                    RunItem::new(SyncRuleRunAction::Copy, side.other(), aside.clone()),
                ]);
            }
            done.conflict_copies
                .extend([aside_a.clone(), aside_b.clone()]);
            done.counts.conflicts += 1;
            done.counts.copied += 2;
        }
        Step::Adopt { path, hash } => {
            let (a, b) = (
                target.listing_a.files.get(path.as_str()),
                target.listing_b.files.get(path.as_str()),
            );
            let row = StateRow {
                path: path.clone(),
                a: a.map(seen),
                b: b.map(seen),
                kept: false,
                hash: Some(hash.clone()),
            };
            write(target, move |tx, device, rule| {
                store::put_state(tx, device, rule, &row)
            })
            .await?;
            done.items.push(RunItem::new(
                SyncRuleRunAction::Adopt,
                SyncRuleSide::A,
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
                    a: None,
                    b: target.listing_b.files.get(path.as_str()).map(seen),
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
        SyncRuleSide::A => row.a = value,
        SyncRuleSide::B => row.b = value,
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
) -> Result<Facts, FilesError> {
    let place = target.place(side);
    place.rename(path, aside).await?;
    place.stat(aside).await?.ok_or_else(changed_meanwhile)
}

/// Both exclusion files go aside and to the other side; the union of their patterns becomes the
/// exclusion file of both places (FR-039).
async fn merge_ignore(target: &Target<'_>, aside_a: &str, aside_b: &str) -> Result<(), StepError> {
    let (a, b) = (target.place(SyncRuleSide::A), target.place(SyncRuleSide::B));
    let text_a = a.read_text(IGNORE_FILE, MAX_BYTES).await?;
    let text_b = b.read_text(IGNORE_FILE, MAX_BYTES).await?;
    let (Some(text_a), Some(text_b)) = (text_a, text_b) else {
        return Err(changed_meanwhile().into());
    };
    let union = union_text(&text_a, &text_b);
    // Keep both originals live until their conflict copies exist on both places. If writing either
    // union fails, the next run can still read both exclusion sets from the original files.
    let set_a = preserve_ignore(target, SyncRuleSide::A, &text_a, aside_a).await?;
    let set_b = preserve_ignore(target, SyncRuleSide::B, &text_b, aside_b).await?;
    copy(target, SyncRuleSide::A, aside_a, &set_a, false).await?;
    copy(target, SyncRuleSide::B, aside_b, &set_b, false).await?;
    a.write_text(IGNORE_FILE, &union).await?;
    b.write_text(IGNORE_FILE, &union).await?;
    let row = StateRow {
        path: IGNORE_FILE.to_owned(),
        a: a.stat(IGNORE_FILE).await?.as_ref().map(seen),
        b: b.stat(IGNORE_FILE).await?.as_ref().map(seen),
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
) -> Result<Facts, FilesError> {
    let place = target.place(side);
    if place.stat(aside).await?.is_some() {
        return Err(changed_meanwhile());
    }
    place.write_text(aside, text).await?;
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
        a: None,
        b: None,
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

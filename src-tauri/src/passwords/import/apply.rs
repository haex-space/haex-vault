//! Writing an import (spec 034, US7, research R12): in steps, so that no transaction grows with the
//! file. First the folders, then **each entry in a write of its own** (with its tags, custom
//! fields, passkeys and history states), then **each attachment in a write of its own**. Every row
//! created goes into an in-memory ledger; a fatal error or a cancel removes what the ledger holds
//! again, entries first and the binaries the import inserted last (phase `rollback`). A problem at
//! one place does not stop the run, it goes to the report.
//!
//! The in-memory ledger is the plain choice for a first version. A crash in the middle leaves the
//! rows written so far; duplicate detection (title, user name, address) makes a repeated run skip
//! them. ponytail: ceiling is a crash mid-import (the user deletes the leftovers or re-runs with
//! "skip duplicates"); upgrade path is a `_no_sync` table of import runs that a restart can finish
//! rolling back.
//!
//! Maintainability exception (spaex 500-LoC rule): the step control, the ledger and the per-row
//! writers form one write pipeline whose rollback must know every row the writers create, so they
//! have stayed together. Concrete split plan, if this grows further: move the per-row writers
//! (`write_groups`, `existing_group`, `write_item`, `link_group`, `write_state`,
//! `write_attachment`, `otp_columns`, `icon_text`) into `apply_rows.rs` next to
//! `apply_extras.rs`, leaving `run`, `write_all`, the ledger and `rollback` here.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::apply_extras::{self, Extras};
use super::report::{folder_path, ReportBuilder};
use super::{
    duplicate_key, failed, ExistingKeys, IconRef, ImportAttachment, ImportItem, ImportModel,
    ImportState, Problem,
};
use crate::error::{HaexError, Result};
use crate::passwords::model::{AttentionKind, ImportReport};
use crate::passwords::passkeys::{self, InsertOutcome};
use crate::passwords::snapshots::{SnapshotAttachment, SnapshotData};
use crate::passwords::{
    binaries, clock, tags, totp, trash, ATTACHMENT_LIMIT_BYTES, TRASH_GROUP_ID,
};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// What to do with an entry the vault already has (FR-023).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "lowercase")]
pub enum OnDuplicate {
    Skip,
    Create,
}

/// The phase a progress event reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Groups,
    Items,
    Attachments,
    Rollback,
}

/// The payload of `passwords-import-progress`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: u32,
    pub total: u32,
    pub phase: Phase,
}

/// A step of a run, for the tests that inject a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Group,
    Item(usize),
    Extras,
    Attachment(usize),
}

/// How a run is controlled from outside: the cancel flag, and (for tests) a fault injector that may
/// fail a step before it is written.
pub struct Control<'a> {
    pub cancel: &'a AtomicBool,
    pub progress: &'a (dyn Fn(Progress) + Send + Sync),
    pub inject: Option<&'a (dyn Fn(Step) -> Option<HaexError> + Send + Sync)>,
}

/// What the run created, in order. Rows go in only after their write committed.
#[derive(Default)]
struct Ledger {
    groups: Vec<String>,
    items: Vec<String>,
    binaries: Vec<String>,
    tag_colors: Vec<(String, Option<String>)>,
    passkeys: Vec<String>,
    presets: Vec<String>,
    trash_created: bool,
    /// The tags the vault had before the run; a tag that was not there and has no entry any more is
    /// the run's and goes with the rollback.
    initial_tags: HashSet<String>,
}

/// An attachment still to write once its entry exists.
struct Job {
    item_id: String,
    file_name: String,
    bytes: Vec<u8>,
    /// The history state this attachment belongs to, for one of a state; `None` for the current one.
    /// The link row waits for the binary (the table points at it), so it is written here.
    snapshot_id: Option<String>,
    /// The last write of an entry takes the snapshot of its current state.
    last_current: bool,
}

struct ItemOutcome {
    id: String,
    /// The ids of the history states written, in the order of the model.
    state_ids: Vec<String>,
    new_binaries: Vec<String>,
    problems: Vec<Problem>,
}

/// Writes the model. On a fatal error or a cancel the ledger is rolled back and the error returned
/// (`ImportFailed { reason: "cancelled" }` for a cancel).
pub async fn run(
    db: &VaultDb,
    mut model: ImportModel,
    existing: &ExistingKeys,
    on_duplicate: OnDuplicate,
    control: &Control<'_>,
) -> Result<ImportReport> {
    let initial_tags = db
        .read(|q| {
            q.query_map("SELECT id FROM haex_passwords_tags", params![], |r| {
                r.get::<_, String>(0)
            })
            .map(|ids| ids.into_iter().collect::<HashSet<_>>())
        })
        .await?;
    let mut ledger = Ledger {
        initial_tags,
        ..Ledger::default()
    };
    match write_all(db, &mut model, existing, on_duplicate, control, &mut ledger).await {
        Ok(report) => Ok(report),
        Err(error) => {
            rollback(db, &ledger, control).await;
            Err(error)
        }
    }
}

fn check(control: &Control<'_>, step: Step) -> Result<()> {
    if control.cancel.load(Ordering::SeqCst) {
        return Err(failed("cancelled"));
    }
    if let Some(inject) = control.inject {
        if let Some(error) = inject(step) {
            return Err(error);
        }
    }
    Ok(())
}

async fn write_all(
    db: &VaultDb,
    model: &mut ImportModel,
    existing: &ExistingKeys,
    on_duplicate: OnDuplicate,
    control: &Control<'_>,
    ledger: &mut Ledger,
) -> Result<ImportReport> {
    let skipped: Vec<bool> = model
        .items
        .iter()
        .map(|item| {
            on_duplicate == OnDuplicate::Skip
                && existing.contains(&duplicate_key(
                    item.title.as_deref(),
                    item.username.as_deref(),
                    item.url.as_deref(),
                ))
        })
        .collect();
    let attachment_total: usize = model
        .items
        .iter()
        .zip(&skipped)
        .filter(|(_, skip)| !**skip)
        .map(|(item, _)| {
            item.attachments.len()
                + item
                    .history
                    .iter()
                    .map(|s| s.attachments.len())
                    .sum::<usize>()
        })
        .sum();
    let total = (model.groups.len() + model.items.len() + attachment_total) as u32;
    let mut done = 0u32;
    let mut report = ReportBuilder::default();
    let progress = |done: u32, phase: Phase| (control.progress)(Progress { done, total, phase });

    // Folders, in one write: they are few and small.
    check(control, Step::Group)?;
    let needs_trash =
        model.groups.iter().any(|g| g.is_recycle_bin) || model.items.iter().any(|i| i.trashed);
    let groups = model.groups.clone();
    let group_result = db
        .write(move |tx| write_groups(tx, &groups, needs_trash).map_err(Into::into))
        .await?;
    ledger.groups = group_result.created;
    ledger.binaries.extend(group_result.new_binaries);
    ledger.trash_created = group_result.trash_created;
    done += model.groups.len() as u32;
    progress(done, Phase::Groups);
    let group_ids = Arc::new(group_result.ids);

    // Spec 036, FR-050: the ids are given before writing, so KeePass references become haex-vault
    // placeholders in the first state of each entry.
    let (converted, left_as_text) = super::apply_references::prepare(&mut model.items, &skipped);
    report.references_converted = converted;
    report.references_left_as_text = left_as_text;

    // Entries, each in a write of its own.
    let mut jobs: Vec<Job> = Vec::new();
    let items = std::mem::take(&mut model.items);
    for (index, mut item) in items.into_iter().enumerate() {
        if skipped[index] {
            report.skipped_duplicates += 1;
            done += 1;
            progress(done, Phase::Items);
            continue;
        }
        check(control, Step::Item(index))?;
        let attachments = std::mem::take(&mut item.attachments);
        let mut history_attachments: Vec<Vec<ImportAttachment>> = Vec::new();
        for state in &mut item.history {
            history_attachments.push(std::mem::take(&mut state.attachments));
        }
        let has_current = !attachments.is_empty();
        let to_write = item.clone();
        let ids = Arc::clone(&group_ids);
        let states_with_files = history_attachments.clone();
        let outcome = db
            .write(move |tx| {
                write_item(tx, &to_write, &ids, &states_with_files, !has_current)
                    .map_err(Into::into)
            })
            .await?;
        ledger.items.push(outcome.id.clone());
        ledger.binaries.extend(outcome.new_binaries);
        let path = folder_path(model, item.group_ref.as_deref());
        let title = item.title.clone().unwrap_or_default();
        for problem in item.problems.iter().chain(outcome.problems.iter()) {
            report.add(Some(&outcome.id), &title, &path, problem);
        }
        report.imported += 1;
        if item.trashed {
            report.trashed += 1;
        }
        report.history_states += item.history.len() as u32;
        for (files, snapshot_id) in history_attachments.into_iter().zip(&outcome.state_ids) {
            for file in files {
                jobs.push(Job {
                    item_id: outcome.id.clone(),
                    file_name: file.file_name,
                    bytes: file.bytes,
                    snapshot_id: Some(snapshot_id.clone()),
                    last_current: false,
                });
            }
        }
        let count = attachments.len();
        for (position, file) in attachments.into_iter().enumerate() {
            jobs.push(Job {
                item_id: outcome.id.clone(),
                file_name: file.file_name,
                bytes: file.bytes,
                snapshot_id: None,
                last_current: position + 1 == count,
            });
        }
        done += 1;
        progress(done, Phase::Items);
    }

    // Tag colours, passkeys of no entry and generator presets, in one write.
    check(control, Step::Extras)?;
    let extras = Extras {
        tag_colors: std::mem::take(&mut model.tag_colors),
        passkeys: std::mem::take(&mut model.passkeys),
        presets: std::mem::take(&mut model.presets),
    };
    let outcome = db
        .write(move |tx| apply_extras::write(tx, &extras).map_err(Into::into))
        .await?;
    ledger.passkeys = outcome.passkeys;
    ledger.presets = outcome.presets;
    ledger.tag_colors = outcome.tag_colors;
    for problem in &outcome.problems {
        report.add(None, "", "", problem);
    }

    // Attachments, each in a write of its own.
    for (position, job) in jobs.into_iter().enumerate() {
        check(control, Step::Attachment(position))?;
        if job.bytes.len() as u64 > ATTACHMENT_LIMIT_BYTES {
            // The parsers already drop these; this keeps the limit even for a model built by hand.
            return Err(HaexError::PasswordsAttachmentTooLarge {
                bytes: job.bytes.len() as u64,
                limit: ATTACHMENT_LIMIT_BYTES,
            });
        }
        let inserted = db
            .write(move |tx| write_attachment(tx, &job).map_err(Into::into))
            .await?;
        if let Some(hash) = inserted {
            ledger.binaries.push(hash);
        }
        done += 1;
        progress(done, Phase::Attachments);
    }
    for problem in &model.source_problems {
        report.add(None, "", "", problem);
    }
    Ok(report.finish())
}

struct GroupResult {
    ids: HashMap<String, String>,
    created: Vec<String>,
    new_binaries: Vec<String>,
    trash_created: bool,
}

fn exists(tx: &mut CrdtTransaction<'_>, sql: &str, id: &str) -> Result<bool> {
    Ok(tx
        .query_row(sql, params![id], |r| r.get::<_, i64>(0))?
        .unwrap_or(0)
        > 0)
}

/// A picture of the source as the text of the `icon` column: the name of a picture of haex-vault, or
/// `binary:<hash>` for a picture of its own (its bytes become a binary of type `icon`).
fn icon_text(
    tx: &mut CrdtTransaction<'_>,
    icon: Option<&IconRef>,
    new_binaries: &mut Vec<String>,
) -> Result<Option<String>> {
    Ok(match icon {
        None => None,
        Some(IconRef::Standard(name)) => Some(name.clone()),
        Some(IconRef::Custom(bytes)) => {
            let hash = binaries::hash_bytes(bytes);
            if binaries::ensure_binary(tx, &hash, bytes, "icon")? {
                new_binaries.push(hash.clone());
            }
            Some(format!("binary:{hash}"))
        }
    })
}

fn write_groups(
    tx: &mut CrdtTransaction<'_>,
    groups: &[super::ImportGroup],
    needs_trash: bool,
) -> Result<GroupResult> {
    let mut result = GroupResult {
        ids: HashMap::new(),
        created: Vec::new(),
        new_binaries: Vec::new(),
        trash_created: false,
    };
    if needs_trash {
        result.trash_created = !exists(
            tx,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = ?1",
            TRASH_GROUP_ID,
        )?;
        trash::ensure_trash(tx)?;
    }
    let now = clock::now();
    for group in groups {
        if group.is_recycle_bin {
            result
                .ids
                .insert(group.reference.clone(), TRASH_GROUP_ID.to_string());
            continue;
        }
        let parent = group
            .parent_ref
            .as_ref()
            .and_then(|r| result.ids.get(r))
            .cloned();
        // A repeated import must not duplicate the tree (spec 037 FR-017): a folder the vault
        // already had with this name at this place is used, and stays out of the ledger.
        if let Some(id) = existing_group(tx, &group.name, parent.as_deref(), &result.created)? {
            result.ids.insert(group.reference.clone(), id);
            continue;
        }
        let id = Uuid::new_v4().to_string();
        let previous = group
            .previous_parent_ref
            .as_ref()
            .and_then(|r| result.ids.get(r))
            .cloned();
        let icon = icon_text(tx, group.icon.as_ref(), &mut result.new_binaries)?;
        tx.execute(
            "INSERT INTO haex_passwords_groups \
             (id, name, description, icon, color, sort_order, parent_id, trashed_from_parent_id, \
              created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                id,
                group.name,
                group.description,
                icon,
                group.color,
                group.sort_order,
                parent,
                previous,
                now
            ],
        )?;
        result.ids.insert(group.reference.clone(), id.clone());
        result.created.push(id);
    }
    Ok(result)
}

/// A folder the vault had before this run with exactly this name under this parent (`None`: at
/// the top). Folders the run created itself are not matched, so two equal siblings of one source
/// stay two.
fn existing_group(
    tx: &mut CrdtTransaction<'_>,
    name: &str,
    parent: Option<&str>,
    created: &[String],
) -> Result<Option<String>> {
    let ids: Vec<String> = tx.query_map(
        "SELECT id FROM haex_passwords_groups \
         WHERE name = ?1 AND COALESCE(parent_id, '') = COALESCE(?2, '') AND id <> ?3 \
         ORDER BY created_at, id",
        params![name, parent, TRASH_GROUP_ID],
        |r| r.get(0),
    )?;
    Ok(ids.into_iter().find(|id| !created.contains(id)))
}

/// The current state of an entry in the columns of `item_details`: a valid TOTP in its normal
/// form, an invalid one exactly as found (R12 point 9).
fn otp_columns(item: &ImportItem) -> (Option<String>, i64, i64, String) {
    let Some(raw) = item.otp_raw.as_deref().filter(|r| !r.trim().is_empty()) else {
        return (None, 6, 30, "SHA1".to_string());
    };
    match totp::resolve(
        raw,
        item.otp_digits,
        item.otp_period,
        item.otp_algorithm.as_deref(),
    ) {
        Ok(params) => (
            Some(params.normalised_secret().to_string()),
            i64::from(params.digits),
            i64::from(params.period),
            params.algorithm.as_str().to_string(),
        ),
        Err(_) => (
            Some(raw.to_string()),
            item.otp_digits.unwrap_or(6),
            item.otp_period.unwrap_or(30),
            item.otp_algorithm
                .clone()
                .unwrap_or_else(|| "SHA1".to_string()),
        ),
    }
}

fn write_item(
    tx: &mut CrdtTransaction<'_>,
    item: &ImportItem,
    group_ids: &HashMap<String, String>,
    state_files: &[Vec<ImportAttachment>],
    snapshot_now: bool,
) -> Result<ItemOutcome> {
    let id = item
        .assigned_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = clock::now();
    let created = item.created_at.clone().unwrap_or_else(|| now.clone());
    let updated = item.updated_at.clone().unwrap_or_else(|| created.clone());
    let mut new_binaries = Vec::new();
    let mut problems = Vec::new();
    let icon = icon_text(tx, item.icon.as_ref(), &mut new_binaries)?;
    let (secret, digits, period, algorithm) = otp_columns(item);
    let aliases = item.autofill_aliases.as_ref().map(|v| v.to_string());
    tx.execute(
        "INSERT INTO haex_passwords_item_details \
         (id, title, username, password, note, icon, color, url, otp_secret, otp_digits, \
          otp_period, otp_algorithm, expires_at, autofill_aliases, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            id,
            item.title,
            item.username,
            item.password,
            item.note,
            icon,
            item.color,
            item.url,
            secret,
            digits,
            period,
            algorithm,
            item.expires_at,
            aliases,
            created,
            updated,
        ],
    )?;
    tags::set_item_tags(tx, &id, &item.tags)?;
    for field in &item.key_values {
        if field.key.trim().is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO haex_passwords_item_key_values (id, item_id, key, value, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                Uuid::new_v4().to_string(),
                id,
                field.key,
                field.value,
                updated
            ],
        )?;
    }
    link_group(tx, item, &id, group_ids)?;
    for passkey in &item.passkeys {
        let mut input = passkey.clone();
        input.item_id = Some(id.clone());
        if passkeys::insert(tx, &input)? == InsertOutcome::Duplicate {
            problems.push(Problem::field(
                AttentionKind::PasskeyDuplicate,
                &format!("passkey {}", passkey.relying_party_id),
            ));
        }
    }
    let mut state_ids = Vec::new();
    for (index, state) in item.history.iter().enumerate() {
        let files = state_files.get(index).map(Vec::as_slice).unwrap_or(&[]);
        state_ids.push(write_state(tx, &id, state, files, &now)?);
    }
    if snapshot_now {
        crate::passwords::snapshots::take_snapshot(tx, &id)?;
    }
    Ok(ItemOutcome {
        id,
        state_ids,
        new_binaries,
        problems,
    })
}

fn link_group(
    tx: &mut CrdtTransaction<'_>,
    item: &ImportItem,
    item_id: &str,
    group_ids: &HashMap<String, String>,
) -> Result<()> {
    let mapped = item
        .group_ref
        .as_ref()
        .and_then(|r| group_ids.get(r))
        .cloned();
    let (group, from) = if item.trashed && mapped.as_deref().is_none_or(|g| g == TRASH_GROUP_ID) {
        let from = item
            .trashed_from_ref
            .as_ref()
            .and_then(|r| group_ids.get(r))
            .cloned();
        (Some(TRASH_GROUP_ID.to_string()), from)
    } else {
        (mapped, None)
    };
    if let Some(group) = group {
        tx.execute(
            "INSERT INTO haex_passwords_group_items (item_id, group_id, trashed_from_group_id) \
             VALUES (?1, ?2, ?3)",
            params![item_id, group, from],
        )?;
    }
    Ok(())
}

/// An earlier state with the time of the source. Its document names the attachments by hash; the
/// binaries and the link rows follow later, each attachment in a write of its own.
fn write_state(
    tx: &mut CrdtTransaction<'_>,
    item_id: &str,
    state: &ImportState,
    files: &[ImportAttachment],
    now: &str,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let mut data: SnapshotData = state.data.clone();
    data.attachments = files
        .iter()
        .map(|f| SnapshotAttachment {
            file_name: binaries::sanitize_file_name(&f.file_name),
            binary_hash: binaries::hash_bytes(&f.bytes),
        })
        .collect();
    let text = serde_json::to_string(&data).map_err(|_| HaexError::CrdtInit {
        reason: "a history state could not be written".to_string(),
    })?;
    let modified = state.modified_at.clone().unwrap_or_else(|| now.to_string());
    tx.execute(
        "INSERT INTO haex_passwords_item_snapshots \
         (id, item_id, snapshot_data, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?4)",
        params![id, item_id, text, modified],
    )?;
    Ok(id)
}

/// One attachment in its own write; returns the hash if its binary row was new.
fn write_attachment(tx: &mut CrdtTransaction<'_>, job: &Job) -> Result<Option<String>> {
    let hash = binaries::hash_bytes(&job.bytes);
    let inserted = binaries::ensure_binary(tx, &hash, &job.bytes, "attachment")?;
    match &job.snapshot_id {
        Some(snapshot_id) => {
            tx.execute(
                "INSERT INTO haex_passwords_snapshot_binaries \
                 (id, snapshot_id, binary_hash, file_name) VALUES (?1, ?2, ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    snapshot_id,
                    hash,
                    binaries::sanitize_file_name(&job.file_name)
                ],
            )?;
        }
        None => {
            binaries::link_attachment(
                tx,
                &job.item_id,
                &job.file_name,
                &hash,
                job.bytes.len() as u64,
            )?;
            if job.last_current {
                crate::passwords::snapshots::take_snapshot(tx, &job.item_id)?;
            }
        }
    }
    Ok(inserted.then_some(hash))
}

/// Removes what the run created: entries (with everything on them) in reverse order, folders,
/// the binaries the run inserted, and the trash row if the run made it. A failing step is logged
/// and the rest goes on, so as much as possible is undone.
async fn rollback(db: &VaultDb, ledger: &Ledger, control: &Control<'_>) {
    let total = (ledger.items.len() + ledger.groups.len() + ledger.binaries.len()) as u32;
    let mut done = 0u32;
    let emit = |done: u32| {
        (control.progress)(Progress {
            done,
            total,
            phase: Phase::Rollback,
        })
    };
    emit(0);
    for chunk in ledger.items.rchunks(200) {
        let ids = chunk.to_vec();
        let count = ids.len() as u32;
        let outcome = db
            .write(move |tx| {
                for id in ids.iter().rev() {
                    trash::purge_item(tx, id)?;
                }
                Ok(())
            })
            .await;
        if let Err(error) = outcome {
            log::warn!("passwords import rollback of entries failed: {error}");
        }
        done += count;
        emit(done);
    }
    let groups = ledger.groups.clone();
    let hashes = ledger.binaries.clone();
    let tag_colors = ledger.tag_colors.clone();
    let passkeys = ledger.passkeys.clone();
    let presets = ledger.presets.clone();
    let trash_created = ledger.trash_created;
    let initial_tags = ledger.initial_tags.clone();
    let group_count = groups.len() as u32;
    let hash_count = hashes.len() as u32;
    let outcome = db
        .write(move |tx| {
            apply_extras::undo(tx, &tag_colors, &passkeys, &presets)?;
            let tag_ids: Vec<String> =
                tx.query_map("SELECT id FROM haex_passwords_tags", params![], |r| {
                    r.get(0)
                })?;
            for id in tag_ids.iter().filter(|id| !initial_tags.contains(*id)) {
                let linked = tx
                    .query_row(
                        "SELECT COUNT(*) FROM haex_passwords_item_tags WHERE tag_id = ?1",
                        params![id],
                        |r| r.get::<_, i64>(0),
                    )?
                    .unwrap_or(0);
                if linked == 0 {
                    tx.execute("DELETE FROM haex_passwords_tags WHERE id = ?1", params![id])?;
                }
            }
            for id in groups.iter().rev() {
                tx.execute(
                    "DELETE FROM haex_passwords_groups WHERE id = ?1",
                    params![id],
                )?;
            }
            for hash in &hashes {
                tx.execute(
                    "DELETE FROM haex_passwords_binaries WHERE hash = ?1",
                    params![hash],
                )?;
            }
            if trash_created {
                tx.execute(
                    "DELETE FROM haex_passwords_groups WHERE id = ?1",
                    params![TRASH_GROUP_ID],
                )?;
            }
            Ok(())
        })
        .await;
    if let Err(error) = outcome {
        log::warn!("passwords import rollback of folders and binaries failed: {error}");
    }
    done += group_count + hash_count;
    emit(done);
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;

//! The second half of a run (spec 051, research R6, R10, FR-025): send the ops to the phone, read
//! back what it holds now, then write holzi, the links, the issues and the run's state in one
//! transaction. A contact whose batch the phone refused keeps its old links and gets an issue; the
//! next run tries again from the same base.

use std::collections::{BTreeMap, BTreeSet};

use haex_crdt::CrdtTransaction;
use tauri_plugin_holzi_android::contacts::{RawData, RawIndex};

use super::gather::{read_all, Deletions, Gathered};
use super::jobs::phone_origin;
use super::mapping::{map_raw, PhoneSide};
use super::reconcile::{next_base, Plan, RawSide};
use super::run::ACTIVE;
use super::sides::{self, Loaded};
use super::store::{self, IssueReason, Link, RunRow, Target};
use super::write::{Batches, Purpose};
use super::{Counts, PhoneBook, RunSummary};
use crate::contacts::{photos, store as contacts_store};
use crate::error::Result;
use crate::vault_gate::VaultDb;

/// A contact with its plan, ready to be written.
pub struct Planned {
    pub contact_id: String,
    pub plan: Plan,
    pub raws: Vec<RawSide>,
    pub loaded: Option<Loaded>,
    pub import: bool,
    pub merge: bool,
    /// The raw contacts' rows in the index, in the order of `raws`.
    pub index: Vec<RawIndex>,
}

/// What the phone answered.
#[derive(Default)]
struct Answers {
    /// Raw contact → version after holzi's write.
    versions: BTreeMap<i64, i64>,
    /// Contact → its new raw contact and that one's version.
    created: BTreeMap<String, (i64, i64)>,
    deleted: BTreeSet<i64>,
    /// Contacts whose batch the phone refused.
    failed: BTreeSet<String>,
}

fn send(book: &dyn PhoneBook, batches: Batches) -> Answers {
    let mut answers = Answers::default();
    for call in batches.calls {
        let result = book.apply(&call.ops);
        let result = match result {
            Ok(result) if result.error.is_none() => result,
            outcome => {
                let reason = match outcome {
                    Ok(result) => result.error.unwrap_or_default(),
                    Err(error) => error.to_string(),
                };
                log::warn!("contacts: the phone refused a batch: {reason}");
                for purpose in &call.purposes {
                    if let Purpose::Change { contact_id } | Purpose::Create { contact_id } = purpose
                    {
                        answers.failed.insert(contact_id.clone());
                    }
                }
                continue;
            }
        };
        for (purpose, answer) in call.purposes.iter().zip(&result.results) {
            match (purpose, answer.raw_contact_id, answer.version) {
                (Purpose::Change { .. }, Some(raw), Some(version)) => {
                    answers.versions.insert(raw, version);
                }
                (Purpose::Create { contact_id }, Some(raw), Some(version)) => {
                    answers.created.insert(contact_id.clone(), (raw, version));
                }
                (Purpose::Delete { raw_contact_id }, ..) => {
                    answers.deleted.insert(*raw_contact_id);
                }
                _ => {}
            }
        }
    }
    answers
}

/// The photo of a contact taken over or merged, if holzi has none (research R10).
fn photo_for(
    book: &dyn PhoneBook,
    item: &Planned,
    data: &BTreeMap<i64, RawData>,
) -> Option<Vec<u8>> {
    if !(item.import || item.merge)
        || item
            .loaded
            .as_ref()
            .is_some_and(|loaded| loaded.photo.is_some())
    {
        return None;
    }
    let raw = item
        .index
        .iter()
        .find(|raw| data.get(&raw.id).is_some_and(|data| data.has_photo))?;
    let bytes = book.photo(raw.id).ok().flatten()?;
    match photos::shrink(&bytes) {
        Ok(jpeg) => Some(jpeg),
        Err(error) => {
            log::warn!("contacts: a photo of the phone was not taken over: {error}");
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn finish(
    db: &VaultDb,
    book: &dyn PhoneBook,
    device: &str,
    gathered: &Gathered,
    planned: Vec<Planned>,
    creates: &[&Target],
    loaded: &BTreeMap<String, Loaded>,
    found: &Deletions,
    data: &BTreeMap<i64, RawData>,
    now: &str,
) -> Result<RunSummary> {
    let mut batches = Batches::default();
    for item in &planned {
        if !item.plan.ops.is_empty() {
            batches.change(&item.contact_id, item.plan.ops.clone());
        }
    }
    let mut missing_account = Vec::new();
    for target in creates {
        let Some(contact) = loaded.get(&target.contact_id) else {
            continue;
        };
        if !gathered.account_writable(&target.account_type, &target.account_name) {
            missing_account.push(target.contact_id.clone());
            continue;
        }
        let account = (target.account_type.as_str(), target.account_name.as_str());
        batches.create(
            &target.contact_id,
            account,
            &contact.side.head,
            &contact.side.values,
        );
    }
    for raw in found.from_holzi.values().flatten() {
        if gathered.index.get(raw).is_some_and(|raw| !raw.deleted) {
            batches.delete(*raw);
        }
    }
    let answers = send(book, batches);

    let mut reread: Vec<i64> = answers.versions.keys().copied().collect();
    reread.extend(answers.created.values().map(|(raw, _)| *raw));
    let after: BTreeMap<i64, PhoneSide> = read_all(book, &reread)?
        .into_iter()
        .map(|(id, raw)| (id, map_raw(&raw.data)))
        .collect();
    let photos: BTreeMap<String, Vec<u8>> = planned
        .iter()
        .filter_map(|item| Some((item.contact_id.clone(), photo_for(book, item, data)?)))
        .collect();

    let state = Written {
        device,
        gathered,
        answers: &answers,
        after: &after,
        now,
    };
    let counts = db.write_blocking(|tx| {
        let mut counts = Counts::default();
        for item in &planned {
            state
                .contact(tx, item, photos.get(&item.contact_id), &mut counts)
                .map_err(haex_crdt::Error::from)?;
        }
        state
            .creates(tx, creates, loaded, &missing_account, &mut counts)
            .map_err(haex_crdt::Error::from)?;
        state
            .deletions(tx, found, &mut counts)
            .map_err(haex_crdt::Error::from)?;
        let row = RunRow {
            state: ACTIVE.to_owned(),
            paused_reason: None,
            last_run_at: Some(now.to_owned()),
            counts: serde_json::to_string(&counts).ok(),
        };
        store::save_run_row(tx, device, &row).map_err(haex_crdt::Error::from)?;
        Ok(counts)
    })?;
    Ok(RunSummary {
        counts,
        paused: None,
        mass_delete: None,
    })
}

/// What the holzi write of a run needs.
struct Written<'a> {
    device: &'a str,
    gathered: &'a Gathered,
    answers: &'a Answers,
    after: &'a BTreeMap<i64, PhoneSide>,
    now: &'a str,
}

impl Written<'_> {
    fn contact(
        &self,
        tx: &mut CrdtTransaction<'_>,
        item: &Planned,
        photo: Option<&Vec<u8>>,
        counts: &mut Counts,
    ) -> Result<()> {
        let id = &item.contact_id;
        if self.answers.failed.contains(id) {
            return store::set_issue(tx, self.device, id, IssueReason::Failed, None, self.now);
        }
        let first = item.index.first();
        if item.import {
            let Some(first) = first else { return Ok(()) };
            sides::create(tx, id, &phone_origin(self.device, first), self.now)?;
        }
        sides::apply(tx, id, &item.plan, self.now)?;
        let mut photo_hash = None;
        if let Some(jpeg) = photo {
            let hash = photos::store(tx, jpeg)?;
            contacts_store::set_photo(tx, id, Some(&hash))?;
            photo_hash = Some(hash);
        }
        if (item.import || item.merge) && self.gathered.target_of(id).is_none() {
            let primary = item
                .index
                .iter()
                .zip(&item.raws)
                .find(|(_, side)| side.writable)
                .map(|(raw, _)| raw)
                .or(first);
            if let Some(raw) = primary {
                let target = Target {
                    contact_id: id.clone(),
                    account_type: raw.account_type.clone(),
                    account_name: raw.account_name.clone(),
                };
                store::set_target(tx, self.device, &target)?;
            }
        }
        let empty = Loaded {
            side: Default::default(),
            updated_at: String::new(),
            photo: None,
        };
        let loaded = item.loaded.as_ref().unwrap_or(&empty);
        let (head, values) = sides::after(&loaded.side, &item.plan);
        let updated_at = if item.import || item.plan.changes_holzi() {
            self.now.to_owned()
        } else {
            loaded.updated_at.clone()
        };
        for (raw, side) in item.index.iter().zip(&item.raws) {
            let phone = self
                .after
                .get(&raw.id)
                .cloned()
                .or_else(|| side.current.clone())
                .unwrap_or_else(|| side.base.phone.clone());
            let previous = self
                .gathered
                .links
                .iter()
                .find(|link| link.raw_contact_id == raw.id);
            let link = Link {
                contact_id: id.clone(),
                raw_contact_id: raw.id,
                lookup_key: raw.lookup_key.clone(),
                account_type: raw.account_type.clone(),
                account_name: raw.account_name.clone(),
                source_id: raw.source_id.clone(),
                version: self
                    .answers
                    .versions
                    .get(&raw.id)
                    .copied()
                    .unwrap_or(raw.version),
                read_only: !side.writable,
                base: next_base(&updated_at, &head, &values, phone, &side.base.mapping),
                photo_base: photo_hash
                    .clone()
                    .or_else(|| previous.and_then(|link| link.photo_base.clone())),
            };
            store::save_link(tx, self.device, &link)?;
        }
        if item.plan.blocked {
            store::set_issue(tx, self.device, id, IssueReason::ReadOnly, None, self.now)?;
        } else {
            store::clear_issue(tx, self.device, id)?;
        }
        if item.import {
            counts.imported += 1;
        } else if item.merge {
            counts.merged += 1;
        } else if item.plan.changes_holzi() {
            counts.updated += 1;
        }
        if !item.plan.ops.is_empty() {
            counts.written += 1;
        }
        Ok(())
    }

    fn creates(
        &self,
        tx: &mut CrdtTransaction<'_>,
        creates: &[&Target],
        loaded: &BTreeMap<String, Loaded>,
        missing_account: &[String],
        counts: &mut Counts,
    ) -> Result<()> {
        for target in creates {
            let id = &target.contact_id;
            if missing_account.contains(id) {
                store::set_issue(
                    tx,
                    self.device,
                    id,
                    IssueReason::AccountRemoved,
                    None,
                    self.now,
                )?;
                continue;
            }
            let (Some(contact), Some((raw, version))) =
                (loaded.get(id), self.answers.created.get(id))
            else {
                if self.answers.failed.contains(id) {
                    store::set_issue(tx, self.device, id, IssueReason::Failed, None, self.now)?;
                }
                continue;
            };
            let phone = self.after.get(raw).cloned().unwrap_or_default();
            let link = Link {
                contact_id: id.clone(),
                raw_contact_id: *raw,
                lookup_key: String::new(),
                account_type: target.account_type.clone(),
                account_name: target.account_name.clone(),
                source_id: None,
                version: *version,
                read_only: false,
                base: next_base(
                    &contact.updated_at,
                    &contact.side.head,
                    &contact.side.values,
                    phone,
                    &BTreeMap::new(),
                ),
                photo_base: None,
            };
            store::save_link(tx, self.device, &link)?;
            store::clear_issue(tx, self.device, id)?;
            counts.written += 1;
        }
        Ok(())
    }

    fn deletions(
        &self,
        tx: &mut CrdtTransaction<'_>,
        found: &Deletions,
        counts: &mut Counts,
    ) -> Result<()> {
        for (contact, raws) in &found.from_phone {
            let photo = contacts_store::photo_of(tx, contact)?;
            contacts_store::delete(tx, contact)?;
            if let Some(hash) = photo {
                photos::mark_if_unused(tx, &hash)?;
            }
            for raw in raws {
                store::forget_raw(tx, *raw)?;
            }
            counts.deleted += 1;
        }
        for raws in found.from_holzi.values() {
            let mut any = false;
            for raw in raws {
                let gone = self.answers.deleted.contains(raw)
                    || !self.gathered.index.get(raw).is_some_and(|raw| !raw.deleted);
                if gone {
                    store::drop_link(tx, self.device, *raw, true)?;
                    any = true;
                }
            }
            if any {
                counts.deleted += 1;
            }
        }
        for raw in &found.unlink {
            store::drop_link(tx, self.device, *raw, true)?;
        }
        Ok(())
    }
}

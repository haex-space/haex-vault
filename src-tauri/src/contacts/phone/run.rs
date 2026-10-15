//! One run of the sync with this phone (spec 051 FR-016, FR-019 to FR-026, research R6 to R9):
//! gather both sides, hold back mass deletions, plan per contact, write the phone, read back what
//! it holds now, then write holzi, the links and the state in one transaction. A batch the phone
//! refuses leaves its contacts with an issue and the rest of the run goes on.

use std::collections::{BTreeMap, BTreeSet};

use tauri_plugin_holzi_android::contacts::RawIndex;
use uuid::Uuid;

use super::apply::Planned;
use super::gather::{
    deletions, gather, linked_contacts, new_groups, read_all, Deletions, Gathered,
};
use super::guard::is_mass_delete;
use super::jobs::{plan_job, Job};
use super::matching::match_groups;
use super::sides::{self, Loaded};
use super::store::{self, RunRow, Target};
use super::{MassDecision, MassDelete, PairDecision, Paused, PhoneBook, ReviewPlan, RunSummary};
use crate::contacts::store as contacts_store;
use crate::error::{HolziError, Result};
use crate::vault_gate::VaultDb;

/// How a run was started.
#[derive(Debug, Clone, Copy)]
pub enum Mode<'a> {
    /// The first run after the overview, with the person's decisions on uncertain pairs.
    First(&'a [PairDecision]),
    /// A run on its own: after opening, on coming back, by hand.
    Normal,
    /// A run because contacts changed in the vault (FR-021). It takes no deletions over from the
    /// phone: a phone that deletes contacts one by one while holzi is open would otherwise be
    /// followed run by run, each below the guard. The next full run sees them all at once.
    Changes,
    /// The person decided on held-back deletions.
    Decided(MassDecision),
}

pub const REVIEW: &str = "review";
pub const ACTIVE: &str = "active";
pub const PAUSED: &str = "paused";

fn off() -> HolziError {
    HolziError::ContactsInvalid {
        reason: "the sync with the phone is off".to_owned(),
    }
}

/// The overview before the first run: nothing is written.
pub fn review(db: &VaultDb, book: &dyn PhoneBook, device: &str) -> Result<ReviewPlan> {
    let gathered = gather(db, book, device)?;
    let groups = new_groups(&gathered);
    let ids: Vec<i64> = groups
        .iter()
        .flat_map(|group| group.raws.iter().map(|raw| raw.id))
        .collect();
    let data = read_all(book, &ids)?;
    let candidates = unlinked_headers(db, &gathered)?;
    let matched = match_groups(&groups, &data, &candidates, Some(&[]));
    let merged_contacts: BTreeSet<&String> = matched.merges.values().collect();
    let to_phone = gathered
        .targets
        .iter()
        .filter(|target| gathered.links_of(&target.contact_id).next().is_none())
        .filter(|target| !merged_contacts.contains(&target.contact_id))
        .count();
    let fresh = groups.iter().filter(|group| group.joins.is_none()).count();
    Ok(ReviewPlan {
        to_address_book: count(fresh - matched.merges.len()),
        to_phone: count(to_phone),
        merged: count(matched.merges.len()),
        uncertain: matched.pairs,
    })
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The headers of contacts without a raw contact on this phone.
fn unlinked_headers(
    db: &VaultDb,
    gathered: &Gathered,
) -> Result<Vec<crate::contacts::model::ContactHeader>> {
    let linked: BTreeSet<String> = gathered
        .links
        .iter()
        .map(|link| link.contact_id.clone())
        .collect();
    let mut headers = db.read_blocking(|q| contacts_store::headers(q).map_err(Into::into))?;
    headers.retain(|header| !linked.contains(&header.id));
    Ok(headers)
}

/// Runs the sync once.
pub fn run(
    db: &VaultDb,
    book: &dyn PhoneBook,
    device: &str,
    mode: Mode<'_>,
    now: &str,
) -> Result<RunSummary> {
    let row = db
        .read_blocking(|q| store::run_row(q, device).map_err(Into::into))?
        .ok_or_else(off)?;
    let paused = row.paused_reason.as_deref().and_then(Paused::parse);
    match (mode, row.state.as_str(), paused) {
        (Mode::First(_), REVIEW, _) => {}
        (Mode::First(_), ..) => return Err(off()),
        (Mode::Normal | Mode::Changes, REVIEW, _) => return Ok(RunSummary::default()),
        (Mode::Normal | Mode::Changes, PAUSED, Some(Paused::MassDelete)) => {
            return Ok(RunSummary {
                paused: Some(Paused::MassDelete),
                mass_delete: row
                    .counts
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok()),
                ..RunSummary::default()
            })
        }
        (Mode::Decided(_), PAUSED, Some(Paused::MassDelete)) => {}
        (Mode::Decided(_), ..) => return Err(off()),
        _ => {}
    }
    if !book.permitted()? {
        if row.state != REVIEW {
            save_state(db, device, PAUSED, Some(Paused::Permission), &row)?;
        }
        return Ok(RunSummary {
            paused: Some(Paused::Permission),
            ..RunSummary::default()
        });
    }
    let gathered = gather(db, book, device)?;
    let mut found = deletions(&gathered);
    if matches!(mode, Mode::Changes) {
        found.from_phone.clear();
    }
    let linked = linked_contacts(&gathered).len();
    if found.count() > 0 && is_mass_delete(found.count(), linked) {
        match mode {
            Mode::Decided(MassDecision::Delete) => {}
            Mode::Decided(MassDecision::Keep) => {
                keep(db, device, &found)?;
                save_state(db, device, ACTIVE, None, &row)?;
                return run(db, book, device, Mode::Normal, now);
            }
            _ => {
                let held = MassDelete {
                    from_phone: count(found.from_phone.len()),
                    from_holzi: count(found.from_holzi.len()),
                    linked: count(linked),
                };
                let paused_row = RunRow {
                    state: PAUSED.to_owned(),
                    paused_reason: Some(Paused::MassDelete.as_str().to_owned()),
                    last_run_at: row.last_run_at.clone(),
                    counts: serde_json::to_string(&held).ok(),
                };
                let device = device.to_owned();
                db.write_blocking(move |tx| {
                    store::save_run_row(tx, &device, &paused_row).map_err(Into::into)
                })?;
                return Ok(RunSummary {
                    paused: Some(Paused::MassDelete),
                    mass_delete: Some(held),
                    ..RunSummary::default()
                });
            }
        }
    }
    let decisions = match mode {
        Mode::First(decisions) => Some(decisions),
        _ => None,
    };
    sync(db, book, device, &gathered, found, decisions, now)
}

fn save_state(
    db: &VaultDb,
    device: &str,
    state: &str,
    paused: Option<Paused>,
    row: &RunRow,
) -> Result<()> {
    let next = RunRow {
        state: state.to_owned(),
        paused_reason: paused.map(|reason| reason.as_str().to_owned()),
        last_run_at: row.last_run_at.clone(),
        counts: row.counts.clone(),
    };
    let device = device.to_owned();
    db.write_blocking(move |tx| store::save_run_row(tx, &device, &next).map_err(Into::into))
}

/// "Keep" on held-back deletions (FR-016): contacts the phone deleted stay in holzi and go to the
/// phone again (they keep their target, their links go); raw contacts whose contact holzi deleted
/// stay on the phone and come back into holzi (the phone forgets it linked them).
fn keep(db: &VaultDb, device: &str, found: &Deletions) -> Result<()> {
    let device = device.to_owned();
    let raws: Vec<i64> = found
        .from_phone
        .values()
        .chain(found.from_holzi.values())
        .flatten()
        .copied()
        .collect();
    db.write_blocking(move |tx| {
        for raw in raws {
            store::drop_link(tx, &device, raw, true)?;
        }
        Ok(())
    })
}

#[allow(clippy::too_many_arguments)]
fn sync(
    db: &VaultDb,
    book: &dyn PhoneBook,
    device: &str,
    gathered: &Gathered,
    found: Deletions,
    decisions: Option<&[PairDecision]>,
    now: &str,
) -> Result<RunSummary> {
    let groups = new_groups(gathered);
    let mut to_read: Vec<i64> = gathered
        .classified
        .changed
        .iter()
        .map(|raw| raw.id)
        .collect();
    to_read.extend(
        groups
            .iter()
            .flat_map(|group| group.raws.iter().map(|raw| raw.id)),
    );
    // Raw contacts kept for a contact that may be a target again (see `kept_raws`).
    to_read.extend(gathered.targets.iter().flat_map(|target| {
        kept_raws(gathered, &target.contact_id)
            .into_iter()
            .map(|raw| raw.id)
    }));
    let data = read_all(book, &to_read)?;
    let deleted: BTreeSet<&String> = found
        .from_phone
        .keys()
        .chain(found.from_holzi.keys())
        .chain(found.untargeted.keys())
        .collect();

    // Links this run drops count as gone already: their contacts go to the phone again, or show
    // why they cannot (an account that left the phone).
    let dropping: BTreeSet<i64> = found.unlink.iter().copied().collect();
    let unlinked_targets: Vec<&Target> = gathered
        .targets
        .iter()
        .filter(|target| {
            gathered
                .links_of(&target.contact_id)
                .all(|link| dropping.contains(&link.raw_contact_id))
        })
        .filter(|target| gathered.updated_at.contains_key(&target.contact_id))
        .collect();
    let candidates = match decisions {
        Some(_) => unlinked_headers(db, gathered)?,
        None => {
            let targeted: BTreeSet<&str> = unlinked_targets
                .iter()
                .map(|target| target.contact_id.as_str())
                .collect();
            let mut headers = unlinked_headers(db, gathered)?;
            headers.retain(|header| targeted.contains(header.id.as_str()));
            headers
        }
    };
    let matched = match_groups(&groups, &data, &candidates, decisions);

    let mut jobs: BTreeMap<String, Job> = BTreeMap::new();
    let changed_contacts: BTreeSet<&str> = gathered
        .classified
        .changed
        .iter()
        .filter_map(|raw| {
            gathered
                .links
                .iter()
                .find(|link| link.raw_contact_id == raw.id)
        })
        .map(|link| link.contact_id.as_str())
        .collect();
    for link in &gathered.links {
        let Some(updated_at) = gathered.updated_at.get(&link.contact_id) else {
            continue;
        };
        let dirty = *updated_at != link.base.holzi_updated_at;
        if (dirty || changed_contacts.contains(link.contact_id.as_str()))
            && !deleted.contains(&link.contact_id)
        {
            jobs.entry(link.contact_id.clone())
                .or_insert_with(|| Job::Linked {
                    contact_id: link.contact_id.clone(),
                    joined: Vec::new(),
                });
        }
    }
    for group in &groups {
        if let Some(contact_id) = &group.joins {
            if deleted.contains(contact_id) {
                continue;
            }
            let job = jobs
                .entry(contact_id.clone())
                .or_insert_with(|| Job::Linked {
                    contact_id: contact_id.clone(),
                    joined: Vec::new(),
                });
            if let Job::Linked { joined, .. } = job {
                joined.extend(group.raws.iter().cloned());
            }
        } else if let Some(contact_id) = matched.merges.get(&group.key) {
            match jobs.get_mut(contact_id) {
                Some(Job::Merge { raws, .. }) => raws.extend(group.raws.iter().cloned()),
                _ => {
                    jobs.insert(
                        contact_id.clone(),
                        Job::Merge {
                            contact_id: contact_id.clone(),
                            raws: group.raws.clone(),
                        },
                    );
                }
            }
        } else {
            let contact_id = Uuid::new_v4().to_string();
            jobs.insert(
                contact_id.clone(),
                Job::Import {
                    contact_id,
                    raws: group.raws.clone(),
                },
            );
        }
    }
    let merged: BTreeSet<String> = jobs
        .values()
        .filter_map(|job| match job {
            Job::Merge { contact_id, .. } => Some(contact_id.clone()),
            _ => None,
        })
        .collect();
    let mut creates: Vec<&Target> = unlinked_targets
        .into_iter()
        .filter(|target| {
            !merged.contains(&target.contact_id) && !deleted.contains(&target.contact_id)
        })
        .collect();
    // A contact that is a target again finds the raw contacts this phone kept for it (FR-018d
    // "keep") and is merged with them instead of written a second time.
    creates.retain(|target| {
        let kept = kept_raws(gathered, &target.contact_id);
        if kept.is_empty() {
            return true;
        }
        jobs.insert(
            target.contact_id.clone(),
            Job::Merge {
                contact_id: target.contact_id.clone(),
                raws: kept,
            },
        );
        false
    });

    // holzi's side of every contact the run touches.
    let wanted: Vec<String> = jobs
        .values()
        .filter_map(|job| match job {
            Job::Linked { contact_id, .. } | Job::Merge { contact_id, .. } => {
                Some(contact_id.clone())
            }
            Job::Import { .. } => None,
        })
        .chain(creates.iter().map(|target| target.contact_id.clone()))
        .collect();
    let loaded: BTreeMap<String, Loaded> = db.read_blocking(move |q| {
        let mut out = BTreeMap::new();
        for id in wanted {
            if let Some(found) = sides::load(q, &id)? {
                out.insert(id, found);
            }
        }
        Ok(out)
    })?;

    let planned: Vec<Planned> = jobs
        .into_values()
        .filter_map(|job| plan_job(job, gathered, &data, &loaded))
        .collect();
    super::apply::finish(
        db, book, device, gathered, planned, &creates, &loaded, &found, &data, now,
    )
}

/// The raw contacts on the phone this phone once linked to a contact and no longer does.
fn kept_raws(gathered: &Gathered, contact_id: &str) -> Vec<RawIndex> {
    gathered
        .classified
        .new
        .iter()
        .filter(|raw| gathered.known.get(&raw.id).map(String::as_str) == Some(contact_id))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "run_targets_tests.rs"]
mod targets_tests;

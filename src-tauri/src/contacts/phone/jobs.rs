//! What a run does with each contact (spec 051, research R7, R8): a contact linked on this phone,
//! a new contact of the phone merged into one of holzi, or one taken over; and the plan of each.

use std::collections::BTreeMap;

use tauri_plugin_holzi_android::contacts::{RawData, RawIndex};

use super::apply::Planned;
use super::gather::Gathered;
use super::mapping::{map_raw, PhoneSide};
use super::reconcile::{next_base, reconcile, HolziSide, RawSide};
use super::sides::Loaded;
use super::store::{Base, Target};
use crate::contacts::model::{Origin, OriginKind};

/// A contact the run works on and how.
pub enum Job {
    /// A contact with raw contacts on this phone; new raw contacts of its aggregated contact join.
    Linked {
        contact_id: String,
        joined: Vec<RawIndex>,
    },
    /// A new contact of the phone merged into a contact (first run, or a clear match to a target).
    Merge {
        contact_id: String,
        raws: Vec<RawIndex>,
    },
    /// A new contact of the phone that becomes a contact.
    Import {
        contact_id: String,
        raws: Vec<RawIndex>,
    },
}

fn raw_side(
    raw: &RawIndex,
    gathered: &Gathered,
    base: Base,
    current: Option<PhoneSide>,
) -> RawSide {
    RawSide {
        raw_contact_id: raw.id,
        writable: gathered.writable(raw),
        base,
        current,
        last_updated: raw.last_updated,
    }
}

/// The base of a raw contact merged into a contact for the first time: holzi's state as agreed,
/// the phone's head as agreed, and no phone values (each one is new).
fn merge_base(loaded: Option<&Loaded>, current: &PhoneSide) -> Base {
    let mut base = match loaded {
        Some(loaded) => next_base(
            &loaded.updated_at,
            &loaded.side.head,
            &loaded.side.values,
            PhoneSide::default(),
            &BTreeMap::new(),
        ),
        None => Base::default(),
    };
    base.phone = PhoneSide {
        head: current.head.clone(),
        head_rows: current.head_rows.clone(),
        values: Vec::new(),
    };
    base
}

/// The raw contact new values go to: the one in the target's account, else the first writable.
fn primary(raws: &[RawSide], index: &[RawIndex], target: Option<&Target>) -> Option<i64> {
    let in_target = target.and_then(|target| {
        index.iter().find(|raw| {
            raw.account_type == target.account_type
                && raw.account_name == target.account_name
                && raws
                    .iter()
                    .any(|side| side.raw_contact_id == raw.id && side.writable)
        })
    });
    in_target.map(|raw| raw.id).or_else(|| {
        raws.iter()
            .find(|side| side.writable)
            .map(|side| side.raw_contact_id)
    })
}

pub fn plan_job(
    job: Job,
    gathered: &Gathered,
    data: &BTreeMap<i64, RawData>,
    loaded: &BTreeMap<String, Loaded>,
) -> Option<Planned> {
    let current = |raw: &RawIndex| data.get(&raw.id).map(|raw| map_raw(&raw.data));
    let (contact_id, import, merge) = match &job {
        Job::Linked { contact_id, .. } => (contact_id.clone(), false, false),
        Job::Merge { contact_id, .. } => (contact_id.clone(), false, true),
        Job::Import { contact_id, .. } => (contact_id.clone(), true, false),
    };
    let (mut raws, mut index) = (Vec::new(), Vec::new());
    let holzi = loaded.get(&contact_id);
    if !import && holzi.is_none() {
        return None;
    }
    if !import {
        for link in gathered.links_of(&contact_id) {
            let Some(raw) = gathered
                .index
                .get(&link.raw_contact_id)
                .filter(|raw| !raw.deleted)
            else {
                continue;
            };
            let read = gathered
                .classified
                .changed
                .iter()
                .any(|changed| changed.id == raw.id);
            raws.push(raw_side(
                raw,
                gathered,
                link.base.clone(),
                if read { current(raw) } else { None },
            ));
            index.push(raw.clone());
        }
    }
    let fresh = match job {
        Job::Linked { joined, .. } => joined,
        Job::Merge { raws, .. } | Job::Import { raws, .. } => raws,
    };
    for raw in &fresh {
        let now = current(raw).unwrap_or_default();
        let base = if import {
            Base::default()
        } else {
            merge_base(holzi, &now)
        };
        raws.push(raw_side(raw, gathered, base, Some(now)));
        index.push(raw.clone());
    }
    if raws.is_empty() {
        return None;
    }
    let target = gathered.target_of(&contact_id);
    let primary = primary(&raws, &index, target);
    let empty = HolziSide::default();
    let side = holzi.map_or(&empty, |loaded| &loaded.side);
    let plan = reconcile(side, &raws, primary, import || merge);
    Some(Planned {
        contact_id,
        plan,
        raws,
        loaded: holzi.cloned(),
        import,
        merge,
        index,
    })
}

/// The origin of a contact taken over from this phone (FR-018a).
pub fn phone_origin(device: &str, raw: &RawIndex) -> Origin {
    Origin {
        kind: OriginKind::Phone,
        device: device.to_owned(),
        caller: "internal:phone".to_owned(),
        account: Some(if raw.account_type.is_empty() {
            String::new()
        } else {
            format!("{}/{}", raw.account_type, raw.account_name)
        }),
    }
}

//! The three-way comparison of one contact (spec 051, research R6, R7): the base of the last run,
//! the raw contacts on the phone and the contact in holzi, per head field and per value.
//!
//! - Changed on one side only: the other side follows.
//! - Changed on both sides to different things: the later change wins; the phone's time is the
//!   aggregated contact's `lastUpdated`, holzi's the HLC time of the field. A missing time, or an
//!   equal one, lets holzi win.
//! - Deleted on one side, changed on the other: the deletion wins (spec 024 FR-017).
//! - Different fields are never weighed against each other; both changes stay.
//!
//! Only kinds that go to a phone take part; holzi addresses, custom values and unknown kinds are
//! neither written to the phone nor touched by it (FR-014). Rows of the phone holzi does not map
//! stay as they are (FR-022). A value changed in holzi goes to the rows that hold it; a new one to
//! the primary raw contact (the one in the target's account, else the first writable one). A
//! contact taken over or merged for the first time runs through the same comparison with a base
//! that has holzi's state and no phone values: every phone value is new then, and holzi's values
//! that the phone lacks go to it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value as Json};
use uuid::Uuid;

use super::mapping::{self, PhoneSide, PhoneValue};
use super::store::{Base, BaseValue};

/// A value of holzi of a kind that goes to a phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HolziValue {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub value: String,
    /// When holzi last changed it (Unix ms).
    pub millis: Option<i64>,
}

/// holzi's side of a contact.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HolziSide {
    pub head: BTreeMap<String, String>,
    /// Head column → when holzi last changed it (Unix ms).
    pub head_millis: BTreeMap<String, i64>,
    pub values: Vec<HolziValue>,
}

/// One raw contact of the contact on this phone.
#[derive(Debug, Clone)]
pub struct RawSide {
    pub raw_contact_id: i64,
    pub writable: bool,
    pub base: Base,
    /// What the phone holds now; `None` when the raw contact did not change since the last run.
    pub current: Option<PhoneSide>,
    /// `lastUpdated` of the aggregated contact (Unix ms).
    pub last_updated: Option<i64>,
}

impl RawSide {
    pub(super) fn now(&self) -> &PhoneSide {
        self.current.as_ref().unwrap_or(&self.base.phone)
    }
}

/// A change of one raw contact's data rows.
#[derive(Debug, Clone, PartialEq)]
pub enum RowOp {
    Insert {
        mimetype: String,
        fields: Map<String, Json>,
    },
    Update {
        data_id: i64,
        mimetype: String,
        fields: Map<String, Json>,
    },
    Delete {
        data_id: i64,
    },
}

/// What the comparison decided for one contact.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// Head columns holzi sets; `None` clears one.
    pub head: Vec<(String, Option<String>)>,
    pub inserts: Vec<HolziValue>,
    /// Value id, label, value.
    pub updates: Vec<(String, String, String)>,
    pub deletes: Vec<String>,
    /// Changes of the phone, per raw contact.
    pub ops: Vec<(i64, RowOp)>,
    /// A change could not go to the phone: no writable raw contact holds or takes it (FR-025).
    pub blocked: bool,
}

impl Plan {
    pub fn changes_holzi(&self) -> bool {
        !(self.head.is_empty()
            && self.inserts.is_empty()
            && self.updates.is_empty()
            && self.deletes.is_empty())
    }
}

fn same(kind: &str, a: (&str, &str), b: (&str, &str)) -> bool {
    a.0 == b.0 && mapping::same_value(kind, a.1, b.1)
}

/// The state of a holzi value or a phone row against the base.
enum Side<'a> {
    Same,
    Changed(&'a str, &'a str),
    Deleted,
}

/// Compares one contact. `fill_head` takes every head field holzi lacks from the phone (a contact
/// taken over or merged for the first time).
pub fn reconcile(
    holzi: &HolziSide,
    raws: &[RawSide],
    primary: Option<i64>,
    fill_head: bool,
) -> Plan {
    let mut plan = Plan::default();
    let writable: BTreeSet<i64> = raws
        .iter()
        .filter(|raw| raw.writable)
        .map(|raw| raw.raw_contact_id)
        .collect();
    let primary = primary.filter(|id| writable.contains(id));
    values(holzi, raws, primary, &writable, &mut plan);
    super::head::head(holzi, raws, primary, &writable, fill_head, &mut plan);
    plan
}

fn values(
    holzi: &HolziSide,
    raws: &[RawSide],
    primary: Option<i64>,
    writable: &BTreeSet<i64>,
    plan: &mut Plan,
) {
    let mut agreed: BTreeMap<&str, &BaseValue> = BTreeMap::new();
    let mut rows_of: BTreeMap<&str, Vec<(&RawSide, i64)>> = BTreeMap::new();
    for raw in raws {
        agreed.extend(
            raw.base
                .holzi_values
                .iter()
                .map(|(id, value)| (id.as_str(), value)),
        );
        for (data_id, value_id) in &raw.base.mapping {
            rows_of
                .entry(value_id.as_str())
                .or_default()
                .push((raw, *data_id));
        }
    }
    let current: BTreeMap<&str, &HolziValue> = holzi
        .values
        .iter()
        .map(|value| (value.id.as_str(), value))
        .collect();
    // Holzi values a phone row already holds, so they need no insert.
    let mut held: BTreeSet<String> = BTreeSet::new();
    // Agreed values no row holds (the phone could not take them before, or a merge just began):
    // they go to the phone unless a new row of the phone holds them.
    let mut pending: Vec<&str> = Vec::new();

    for (id, base) in &agreed {
        let rows = rows_of.get(id).cloned().unwrap_or_default();
        let holzi_side = match current.get(id) {
            None => Side::Deleted,
            Some(now)
                if same(
                    &base.kind,
                    (&now.label, &now.value),
                    (&base.label, &base.value),
                ) =>
            {
                Side::Same
            }
            Some(now) => Side::Changed(&now.label, &now.value),
        };
        let mut phone_side = if rows.is_empty() {
            None
        } else {
            Some(Side::Deleted)
        };
        let mut phone_time = None;
        for (raw, data_id) in &rows {
            let before = raw
                .base
                .phone
                .values
                .iter()
                .find(|row| row.data_id == *data_id);
            match (
                raw.now().values.iter().find(|row| row.data_id == *data_id),
                before,
            ) {
                (None, _) => {}
                (Some(now), Some(before))
                    if !same(
                        &base.kind,
                        (&now.label, &now.value),
                        (&before.label, &before.value),
                    ) =>
                {
                    phone_side = Some(Side::Changed(&now.label, &now.value));
                    phone_time = raw.last_updated;
                }
                (Some(_), _) => {
                    if !matches!(phone_side, Some(Side::Changed(..))) {
                        phone_side = Some(Side::Same);
                    }
                }
            }
        }
        let Some(phone_side) = phone_side else {
            pending.push(id);
            continue;
        };
        held.insert((*id).to_owned());
        match (holzi_side, phone_side) {
            (Side::Same, Side::Same) | (Side::Deleted, Side::Deleted) => {}
            (Side::Same, Side::Changed(label, value)) => {
                plan.updates
                    .push(((*id).to_owned(), label.to_owned(), value.to_owned()));
                write_rows(&base.kind, &rows, label, value, writable, plan);
            }
            (Side::Same | Side::Changed(..), Side::Deleted) => plan.deletes.push((*id).to_owned()),
            (Side::Changed(label, value), Side::Same) => {
                write_rows(&base.kind, &rows, label, value, writable, plan)
            }
            (Side::Changed(label, value), Side::Changed(phone_label, phone_value)) => {
                let holzi_time = current.get(id).and_then(|now| now.millis);
                if same(&base.kind, (label, value), (phone_label, phone_value)) {
                    continue;
                }
                if later(phone_time, holzi_time) {
                    plan.updates.push((
                        (*id).to_owned(),
                        phone_label.to_owned(),
                        phone_value.to_owned(),
                    ));
                    write_rows(&base.kind, &rows, phone_label, phone_value, writable, plan);
                } else {
                    write_rows(&base.kind, &rows, label, value, writable, plan);
                }
            }
            (Side::Deleted, Side::Same | Side::Changed(..)) => {
                for (raw, data_id) in &rows {
                    if raw.now().values.iter().any(|row| row.data_id == *data_id) {
                        delete_row(raw.raw_contact_id, *data_id, writable, plan);
                    }
                }
            }
        }
    }

    // Rows the phone has that the base does not: matched to a holzi value, else new in holzi.
    let mut finals: Vec<(String, String, String)> = holzi
        .values
        .iter()
        .filter(|value| !plan.deletes.contains(&value.id))
        .map(|value| (value.id.clone(), value.kind.clone(), value.value.clone()))
        .collect();
    for raw in raws {
        let known: BTreeSet<i64> = raw
            .base
            .phone
            .values
            .iter()
            .map(|row| row.data_id)
            .collect();
        for row in raw
            .now()
            .values
            .iter()
            .filter(|row| !known.contains(&row.data_id))
        {
            if let Some((id, ..)) = finals.iter().find(|(_, kind, value)| {
                *kind == row.kind && mapping::same_value(kind, value, &row.value)
            }) {
                held.insert(id.clone());
                continue;
            }
            let value = new_value(row);
            finals.push((value.id.clone(), value.kind.clone(), value.value.clone()));
            held.insert(value.id.clone());
            plan.inserts.push(value);
        }
    }

    // Holzi values the phone does not have.
    for value in &holzi.values {
        let new_in_holzi = !agreed.contains_key(value.id.as_str());
        if (new_in_holzi || pending.contains(&value.id.as_str())) && !held.contains(&value.id) {
            insert_on_phone(value, primary, plan);
        }
    }
}

fn new_value(row: &PhoneValue) -> HolziValue {
    HolziValue {
        id: Uuid::new_v4().to_string(),
        kind: row.kind.clone(),
        label: row.label.clone(),
        value: row.value.clone(),
        millis: None,
    }
}

/// Whether the phone's change is later than holzi's; without both times holzi wins.
pub(super) fn later(phone: Option<i64>, holzi: Option<i64>) -> bool {
    matches!((phone, holzi), (Some(phone), Some(holzi)) if phone > holzi)
}

fn insert_on_phone(value: &HolziValue, primary: Option<i64>, plan: &mut Plan) {
    let Some((mimetype, fields)) = mapping::insert_fields(&value.kind, &value.label, &value.value)
    else {
        return;
    };
    match primary {
        Some(raw) => plan.ops.push((
            raw,
            RowOp::Insert {
                mimetype: mimetype.to_owned(),
                fields,
            },
        )),
        None => plan.blocked = true,
    }
}

fn write_rows(
    kind: &str,
    rows: &[(&RawSide, i64)],
    label: &str,
    value: &str,
    writable: &BTreeSet<i64>,
    plan: &mut Plan,
) {
    for (raw, data_id) in rows {
        let Some(now) = raw.now().values.iter().find(|row| row.data_id == *data_id) else {
            continue;
        };
        if same(kind, (&now.label, &now.value), (label, value)) {
            continue;
        }
        if !writable.contains(&raw.raw_contact_id) {
            plan.blocked = true;
            continue;
        }
        let Some(mimetype) = mapping::mimetype_of(kind) else {
            continue;
        };
        let fields = mapping::update_fields(now, label, value);
        plan.ops.push((
            raw.raw_contact_id,
            RowOp::Update {
                data_id: *data_id,
                mimetype: mimetype.to_owned(),
                fields,
            },
        ));
    }
}

fn delete_row(raw: i64, data_id: i64, writable: &BTreeSet<i64>, plan: &mut Plan) {
    if writable.contains(&raw) {
        plan.ops.push((raw, RowOp::Delete { data_id }));
    } else {
        plan.blocked = true;
    }
}

/// The base after a run: holzi's state and what the phone holds now, with each row mapped to the
/// holzi value it holds (kept from before where the value still exists, else by equal value).
pub fn next_base(
    updated_at: &str,
    head: &BTreeMap<String, String>,
    values: &[HolziValue],
    phone: PhoneSide,
    before: &BTreeMap<i64, String>,
) -> Base {
    let ids: BTreeSet<&str> = values.iter().map(|value| value.id.as_str()).collect();
    let mut mapping = BTreeMap::new();
    for row in &phone.values {
        let kept = before
            .get(&row.data_id)
            .filter(|id| ids.contains(id.as_str()));
        let found = kept.cloned().or_else(|| {
            values
                .iter()
                .find(|value| {
                    value.kind == row.kind
                        && mapping::same_value(&row.kind, &value.value, &row.value)
                })
                .map(|value| value.id.clone())
        });
        if let Some(id) = found {
            mapping.insert(row.data_id, id);
        }
    }
    Base {
        holzi_updated_at: updated_at.to_owned(),
        holzi_head: head.clone(),
        holzi_values: values
            .iter()
            .map(|value| {
                let base = BaseValue {
                    kind: value.kind.clone(),
                    label: value.label.clone(),
                    value: value.value.clone(),
                };
                (value.id.clone(), base)
            })
            .collect(),
        phone,
        mapping,
    }
}

#[cfg(test)]
#[path = "reconcile_tests.rs"]
mod tests;

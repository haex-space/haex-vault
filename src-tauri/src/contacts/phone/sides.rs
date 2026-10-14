//! holzi's side of a contact for the comparison with a phone, and the writes of a plan to holzi
//! (spec 051). Runs inside the run's reads and writes; the access checks of the service do not
//! apply, the sync with the phone is holzi's own function (`Caller::Internal`).

use std::collections::BTreeMap;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::reconcile::{HolziSide, HolziValue, Plan};
use super::store::{changed_millis, hlc_millis};
use crate::contacts::kinds;
use crate::contacts::model::{Head, Origin};
use crate::contacts::service::MAX_NOTE_BYTES;
use crate::contacts::store::{self, HEAD_COLUMNS};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

/// A contact of holzi as the run sees it.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub side: HolziSide,
    pub updated_at: String,
    pub photo: Option<String>,
}

fn columns(head: &Head) -> [(&'static str, &Option<String>); 10] {
    [
        ("display_name", &head.display_name),
        ("given_name", &head.given_name),
        ("family_name", &head.family_name),
        ("middle_name", &head.middle_name),
        ("name_prefix", &head.name_prefix),
        ("name_suffix", &head.name_suffix),
        ("nickname", &head.nickname),
        ("organization", &head.organization),
        ("job_title", &head.job_title),
        ("note", &head.note),
    ]
}

/// The head fields with a value.
pub fn head_map(head: &Head) -> BTreeMap<String, String> {
    columns(head)
        .into_iter()
        .filter_map(|(column, value)| {
            let value = value
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())?;
            Some((column.to_owned(), value.to_owned()))
        })
        .collect()
}

/// A head from head fields, cut to holzi's limits (a phone's note can be longer).
pub fn head_of(map: &BTreeMap<String, String>) -> Head {
    let get = |column: &str| {
        map.get(column).map(|value| {
            let limit = if column == "note" {
                MAX_NOTE_BYTES
            } else {
                1024
            };
            cut(value, limit, column == "note")
        })
    };
    Head {
        display_name: get("display_name"),
        given_name: get("given_name"),
        family_name: get("family_name"),
        middle_name: get("middle_name"),
        name_prefix: get("name_prefix"),
        name_suffix: get("name_suffix"),
        nickname: get("nickname"),
        organization: get("organization"),
        job_title: get("job_title"),
        note: get("note"),
    }
}

/// `text` cut to `limit` bytes (`bytes`) or characters, at a character boundary.
fn cut(text: &str, limit: usize, bytes: bool) -> String {
    if bytes {
        let mut end = text.len().min(limit);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].to_owned()
    } else {
        text.chars().take(limit).collect()
    }
}

/// holzi's side of a contact; `None` when it does not exist (any more).
pub fn load(q: &mut impl Query, contact_id: &str) -> Result<Option<Loaded>> {
    let contact = match store::load(q, contact_id) {
        Ok(contact) => contact,
        Err(HolziError::ContactsNotFound) => return Ok(None),
        Err(error) => return Err(error),
    };
    let head_millis = head_millis(q, contact_id)?;
    let mut values = Vec::new();
    for value in contact.values {
        if !(kinds::is_known(&value.kind) && kinds::goes_to_phone(&value.kind)) {
            continue;
        }
        let millis = changed_millis(q, "haex_contacts_values", &value.id, &["label", "value"])?;
        values.push(HolziValue {
            id: value.id,
            kind: value.kind,
            label: value.label,
            value: value.value,
            millis,
        });
    }
    Ok(Some(Loaded {
        side: HolziSide {
            head: head_map(&contact.head),
            head_millis,
            values,
        },
        updated_at: contact.updated_at,
        photo: contact.photo,
    }))
}

fn head_millis(q: &mut impl Query, contact_id: &str) -> Result<BTreeMap<String, i64>> {
    let row: Option<(Option<String>, Option<String>)> = q.query_row(
        "SELECT haex_hlc_no_sync, haex_column_hlcs_no_sync FROM haex_contacts WHERE id = ?1",
        params![contact_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let Some((row_hlc, column_hlcs)) = row else {
        return Ok(BTreeMap::new());
    };
    let by_column: BTreeMap<String, String> = column_hlcs
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    Ok(HEAD_COLUMNS
        .iter()
        .filter_map(|column| {
            let hlc = by_column.get(*column).or(row_hlc.as_ref())?;
            Some(((*column).to_owned(), hlc_millis(hlc)?))
        })
        .collect())
}

/// Inserts a contact taken over from a phone with its head; values follow with [`apply`].
pub fn create(tx: &mut CrdtTransaction<'_>, id: &str, origin: &Origin, now: &str) -> Result<()> {
    store::insert(tx, id, &Head::default(), origin, now)
}

/// Writes what a plan changes in holzi and stamps the contact when anything changed.
pub fn apply(tx: &mut CrdtTransaction<'_>, contact_id: &str, plan: &Plan, now: &str) -> Result<()> {
    if !plan.head.is_empty() {
        let next = head_of(
            &plan
                .head
                .iter()
                .filter_map(|(column, value)| Some((column.clone(), value.clone()?)))
                .collect(),
        );
        for (column, value) in columns(&next) {
            if plan.head.iter().any(|(changed, _)| changed == column) {
                store::set_head(tx, contact_id, column, value.as_deref())?;
            }
        }
    }
    for value in &plan.inserts {
        let position = store::next_position(tx, contact_id, &value.kind)?;
        store::insert_value_as(
            tx,
            &value.id,
            contact_id,
            (&value.kind, &value.label, &value.value),
            position,
        )?;
    }
    for (id, label, value) in &plan.updates {
        store::update_value(tx, id, label, value, None)?;
    }
    for id in &plan.deletes {
        store::delete_value(tx, id)?;
    }
    if plan.changes_holzi() {
        store::touch(tx, contact_id, now)?;
    }
    Ok(())
}

/// holzi's head fields and phone values after a plan.
pub fn after(side: &HolziSide, plan: &Plan) -> (BTreeMap<String, String>, Vec<HolziValue>) {
    let mut head = side.head.clone();
    for (column, value) in &plan.head {
        match value {
            Some(value) => head.insert(column.clone(), value.clone()),
            None => head.remove(column),
        };
    }
    let mut values: Vec<HolziValue> = side
        .values
        .iter()
        .filter(|value| !plan.deletes.contains(&value.id))
        .cloned()
        .map(|mut value| {
            if let Some((_, label, text)) = plan.updates.iter().find(|(id, ..)| *id == value.id) {
                value.label = label.clone();
                value.value = text.clone();
            }
            value
        })
        .collect();
    values.extend(plan.inserts.iter().cloned());
    (head, values)
}

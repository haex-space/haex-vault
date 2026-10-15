//! The ops a run sends to the phone (spec 051, contracts/android-plugin.md): changes of linked raw
//! contacts from the comparison, new raw contacts for contacts that have this phone as a target
//! and no raw contact on it yet (in the target's account, FR-021), and deletions. Ops go in calls
//! of at most 400; an insert and the rows that refer to it always share a call.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json};
use tauri_plugin_holzi_android::contacts::{Op, Target};

use super::mapping::{self, HEAD_FIELDS};
use super::reconcile::{HolziValue, RowOp};

/// Most ops in one call of the phone.
pub const MAX_OPS: usize = 400;

/// What an op is for, so the run knows what the answer belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Purpose {
    /// A change of a linked raw contact of this contact.
    Change { contact_id: String },
    /// The new raw contact of this contact.
    Create { contact_id: String },
    /// A data row of a new raw contact.
    CreateRow,
    /// The deletion of a raw contact.
    Delete { raw_contact_id: i64 },
}

/// Ops with what each one is for; `ref`s are numbered within one call.
#[derive(Debug, Default)]
pub struct Batch {
    pub ops: Vec<Op>,
    pub purposes: Vec<Purpose>,
}

/// Puts ops into calls of at most [`MAX_OPS`], keeping each group in one call.
#[derive(Debug, Default)]
pub struct Batches {
    pub calls: Vec<Batch>,
}

impl Batches {
    fn room(&mut self, needed: usize) -> &mut Batch {
        let fits = self
            .calls
            .last()
            .is_some_and(|last| last.ops.len() + needed <= MAX_OPS);
        if !fits {
            self.calls.push(Batch::default());
        }
        self.calls.last_mut().expect("a call was just added")
    }

    /// The changes of a linked contact's raw contacts.
    pub fn change(&mut self, contact_id: &str, ops: Vec<(i64, RowOp)>) {
        let ops: Vec<Op> = ops
            .into_iter()
            .flat_map(|(raw, op)| row_ops(raw, op))
            .collect();
        for chunk in ops.chunks(MAX_OPS) {
            let batch = self.room(chunk.len());
            for op in chunk {
                batch.ops.push(op.clone());
                batch.purposes.push(Purpose::Change {
                    contact_id: contact_id.to_owned(),
                });
            }
        }
    }

    /// A new raw contact with the contact's head fields and phone values.
    pub fn create(
        &mut self,
        contact_id: &str,
        account: (&str, &str),
        head: &BTreeMap<String, String>,
        values: &[HolziValue],
    ) {
        let mut rows = head_rows(head);
        rows.extend(
            values
                .iter()
                .filter_map(|value| mapping::insert_fields(&value.kind, &value.label, &value.value))
                .map(|(mimetype, fields)| (mimetype.to_owned(), fields)),
        );
        rows.truncate(MAX_OPS - 1);
        let batch = self.room(rows.len() + 1);
        let reference = u32::try_from(batch.ops.len()).unwrap_or(u32::MAX);
        batch.ops.push(Op::InsertRaw {
            account_type: account.0.to_owned(),
            account_name: account.1.to_owned(),
            reference,
        });
        batch.purposes.push(Purpose::Create {
            contact_id: contact_id.to_owned(),
        });
        for (mimetype, fields) in rows {
            batch.ops.push(Op::InsertData {
                raw: Target::Ref(reference),
                mimetype,
                fields,
            });
            batch.purposes.push(Purpose::CreateRow);
        }
    }

    /// The deletion of a raw contact.
    pub fn delete(&mut self, raw_contact_id: i64) {
        let batch = self.room(1);
        batch.ops.push(Op::DeleteRaw { raw_contact_id });
        batch.purposes.push(Purpose::Delete { raw_contact_id });
    }
}

/// The ops of a row change. A membership moves to another group by leaving one row and adding
/// another: the new group's id depends on the raw contact's account (T096).
fn row_ops(raw: i64, op: RowOp) -> Vec<Op> {
    match op {
        RowOp::Update {
            data_id,
            mimetype,
            fields,
        } if mimetype == "GroupMembership" => vec![
            Op::DeleteData { data_id },
            Op::InsertData {
                raw: Target::Id(raw),
                mimetype,
                fields,
            },
        ],
        op => vec![row_op(Target::Id(raw), op)],
    }
}

fn row_op(raw: Target, op: RowOp) -> Op {
    match op {
        RowOp::Insert { mimetype, fields } => Op::InsertData {
            raw,
            mimetype,
            fields,
        },
        RowOp::Update {
            data_id,
            mimetype,
            fields,
        } => Op::UpdateData {
            data_id,
            mimetype,
            fields,
        },
        RowOp::Delete { data_id } => Op::DeleteData { data_id },
    }
}

/// One insert per head mimetype with the fields holzi has.
fn head_rows(head: &BTreeMap<String, String>) -> Vec<(String, Map<String, Json>)> {
    let mut by_mimetype: BTreeMap<&str, BTreeMap<&str, Option<&str>>> = BTreeMap::new();
    for field in &HEAD_FIELDS {
        if let Some(value) = head.get(field.column) {
            by_mimetype
                .entry(field.mimetype)
                .or_default()
                .insert(field.column, Some(value.as_str()));
        }
    }
    by_mimetype
        .into_iter()
        .map(|(mimetype, columns)| {
            (
                mimetype.to_owned(),
                mapping::head_fields(mimetype, &columns),
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;

//! The pass part of a history state (spec 092, FR-032, research R2, data-model.md "History stand").
//! A state of a pass adds an optional block to `SnapshotData`: the kind of pass, the start and the
//! codes with their values. A state of a login has no block. The block is built from, and restored
//! to, the detail row and the codes of the entry; the parent link and the folder are not in it, so
//! a restore never moves or re-links a pass. The codes are secrets: `Debug` prints none of them and
//! the window sees formats and labels, a value only through `reveal`.

use std::fmt;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::model::{SnapshotCodeView, SnapshotPassView};
use crate::error::Result;
use crate::storage::query::Query;

/// One code of a state.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SnapshotCode {
    pub format: String,
    pub value: String,
    pub label: Option<String>,
    pub position: i64,
}

/// The pass block of a state. A start is a date or a moment with the zone of its place.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SnapshotPass {
    pub pass_kind: Option<String>,
    pub start_date: Option<String>,
    pub start_at: Option<String>,
    pub start_zone: Option<String>,
    pub codes: Vec<SnapshotCode>,
}

impl fmt::Debug for SnapshotPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SnapshotPass")
            .field("pass_kind", &self.pass_kind)
            .field("codes", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// The pass block of an entry as it is now: `None` for a login. A pass whose detail row is missing
/// reads as an `other` pass without start and codes (it is repaired at the next write).
pub(super) fn build(q: &mut impl Query, item_id: &str) -> Result<Option<SnapshotPass>> {
    let kind = q
        .query_row(
            "SELECT kind FROM haex_passwords_item_details WHERE id = ?1",
            params![item_id],
            |r| r.get::<_, String>(0),
        )?
        .unwrap_or_default();
    if kind != super::kind::EntryType::PASS {
        return Ok(None);
    }
    let mut pass = q
        .query_row(
            "SELECT pass_kind, start_date, start_at, start_zone \
             FROM haex_passwords_pass_details WHERE item_id = ?1",
            params![item_id],
            |r| {
                Ok(SnapshotPass {
                    pass_kind: r.get(0)?,
                    start_date: r.get(1)?,
                    start_at: r.get(2)?,
                    start_zone: r.get(3)?,
                    codes: Vec::new(),
                })
            },
        )?
        .unwrap_or_else(|| SnapshotPass {
            pass_kind: Some("other".to_owned()),
            ..SnapshotPass::default()
        });
    pass.codes = q.query_map(
        "SELECT format, value, label, position FROM haex_passwords_pass_barcodes \
         WHERE item_id = ?1 ORDER BY position, id",
        params![item_id],
        |r| {
            Ok(SnapshotCode {
                format: r.get(0)?,
                value: r.get(1)?,
                label: r.get(2)?,
                position: r.get(3)?,
            })
        },
    )?;
    Ok(Some(pass))
}

/// The names of what differs in the pass block, in the vocabulary of the window: `kindOfPass`,
/// `start`, `codes`. No block on either side reads as an empty one.
pub(super) fn changed(before: Option<&SnapshotPass>, next: Option<&SnapshotPass>) -> Vec<String> {
    let empty = SnapshotPass::default();
    let (before, next) = (before.unwrap_or(&empty), next.unwrap_or(&empty));
    let mut names = Vec::new();
    if before.pass_kind != next.pass_kind {
        names.push("kindOfPass".to_owned());
    }
    if (&before.start_date, &before.start_at, &before.start_zone)
        != (&next.start_date, &next.start_at, &next.start_zone)
    {
        names.push("start".to_owned());
    }
    if before.codes != next.codes {
        names.push("codes".to_owned());
    }
    names
}

/// What a window shows of the block: formats and labels, never a value.
pub(super) fn view(pass: &SnapshotPass) -> SnapshotPassView {
    SnapshotPassView {
        pass_kind: pass.pass_kind.clone(),
        start_date: pass.start_date.clone(),
        start_at: pass.start_at.clone(),
        start_zone: pass.start_zone.clone(),
        codes: pass
            .codes
            .iter()
            .map(|code| SnapshotCodeView {
                format: code.format.clone(),
                label: code.label.clone(),
                position: code.position,
            })
            .collect(),
    }
}

/// Makes the detail row and the codes those of the state. The parent and the folder stay. A code
/// of the same format and value keeps its row (its label and place are set), so concurrent edits of
/// other codes still merge row by row.
pub(super) fn restore(
    tx: &mut CrdtTransaction<'_>,
    item_id: &str,
    wanted: &SnapshotPass,
) -> Result<()> {
    let has_row = tx
        .query_row(
            "SELECT COUNT(*) FROM haex_passwords_pass_details WHERE item_id = ?1",
            params![item_id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    let kind = wanted
        .pass_kind
        .clone()
        .unwrap_or_else(|| "other".to_owned());
    if has_row {
        tx.execute(
            "UPDATE haex_passwords_pass_details \
             SET pass_kind = ?2, start_date = ?3, start_at = ?4, start_zone = ?5 WHERE item_id = ?1",
            params![item_id, kind, wanted.start_date, wanted.start_at, wanted.start_zone],
        )?;
    } else {
        tx.execute(
            "INSERT INTO haex_passwords_pass_details \
             (item_id, pass_kind, start_date, start_at, start_zone) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                item_id,
                kind,
                wanted.start_date,
                wanted.start_at,
                wanted.start_zone
            ],
        )?;
    }
    let mut existing: Vec<(String, String, String, Option<String>, i64)> = tx.query_map(
        "SELECT id, format, value, label, position FROM haex_passwords_pass_barcodes \
         WHERE item_id = ?1 ORDER BY position, id",
        params![item_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    for code in &wanted.codes {
        let at = existing
            .iter()
            .position(|(_, format, value, _, _)| *format == code.format && *value == code.value);
        match at {
            Some(index) => {
                let (id, _, _, label, position) = existing.remove(index);
                if label != code.label || position != code.position {
                    tx.execute(
                        "UPDATE haex_passwords_pass_barcodes SET label = ?2, position = ?3 \
                         WHERE id = ?1",
                        params![id, code.label, code.position],
                    )?;
                }
            }
            None => {
                tx.execute(
                    "INSERT INTO haex_passwords_pass_barcodes \
                     (id, item_id, format, value, label, position) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        Uuid::new_v4().to_string(),
                        item_id,
                        code.format,
                        code.value,
                        code.label,
                        code.position
                    ],
                )?;
            }
        }
    }
    for (id, ..) in existing {
        tx.execute(
            "DELETE FROM haex_passwords_pass_barcodes WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

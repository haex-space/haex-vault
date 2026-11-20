//! Changes and pages as they travel between the devices of a vault
//! (contracts/sync-protocol.md, research R4, R6).
//!
//! A [`Change`] is haex-crdt's `ColumnChange` without the fields haex-vault does
//! not send: `device_id` (the origin is the node in the HLC) and `sig`
//! (per-change signatures arrive with specs 027/028). The value travels as
//! its JSON text in haex-crdt's encoding, so a BLOB stays a BLOB. postcard
//! cannot carry a `serde_json::Value`, a string it can.
//!
//! A value that alone exceeds a page travels in parts: every part but the
//! last has `continues` set, and the parts of one cell follow each other.

use haex_crdt::rusqlite::ToSql;
use haex_crdt::{serialized_parameter_bytes, ColumnChange, ValueConverter};
use serde::{Deserialize, Serialize};

use crate::sync::progress::{self, Vector};

/// Bytes a page may carry, below the 4 MiB frame limit after the handshake
/// with room for the frame and page overhead.
pub const PAGE_BUDGET: usize = 4 * 1024 * 1024 - 64 * 1024;

/// Per-change overhead [`Change::wire_size`] adds to the field lengths: the
/// postcard length prefixes and the flag.
const CHANGE_OVERHEAD: usize = 24;

/// One cell, or one part of a cell's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub table: String,
    pub row_pks: String,
    pub column: String,
    pub hlc: String,
    /// The value's JSON text, or a part of it when `continues` is set.
    pub value: String,
    /// The value continues in the next change, a part of the same cell.
    pub continues: bool,
}

/// One page of a pull. Pages never share a transaction group, except that a
/// group too large for one page continues in the next with
/// `group_continues` set on every page but its last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    pub changes: Vec<Change>,
    pub group_continues: bool,
    /// Another page follows.
    pub more: bool,
    /// On the last page, the sender's progress the pull was served against:
    /// the receiver holds every change up to it once it applied all pages.
    pub served: Vector,
}

/// Why a change cannot be decoded back into a `ColumnChange`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChangeError {
    #[error("a change value is not valid JSON")]
    Value,
    #[error("a change value does not map to an SQL value")]
    SqlValue,
}

impl Change {
    /// The change as it leaves this device.
    pub fn from_column(change: &ColumnChange) -> Self {
        Self {
            table: change.table_name.clone(),
            row_pks: change.row_pks.clone(),
            column: change.column_name.clone(),
            hlc: change.hlc_timestamp.clone(),
            value: change.value.to_string(),
            continues: false,
        }
    }

    /// The change as haex-crdt applies it; `device_id` names the origin.
    pub fn to_column(&self) -> Result<ColumnChange, ChangeError> {
        Ok(ColumnChange {
            table_name: self.table.clone(),
            row_pks: self.row_pks.clone(),
            column_name: self.column.clone(),
            hlc_timestamp: self.hlc.clone(),
            value: serde_json::from_str(&self.value).map_err(|_| ChangeError::Value)?,
            device_id: progress::origin_of(&self.hlc)
                .map(|origin| origin.to_string())
                .unwrap_or_default(),
            sig: None,
        })
    }

    /// Approximate encoded size, used to fill pages.
    pub fn wire_size(&self) -> usize {
        self.table.len()
            + self.row_pks.len()
            + self.column.len()
            + self.hlc.len()
            + self.value.len()
            + CHANGE_OVERHEAD
    }

    /// Whether `other` is the next part of the same cell.
    fn same_cell(&self, other: &Change) -> bool {
        self.table == other.table
            && self.row_pks == other.row_pks
            && self.column == other.column
            && self.hlc == other.hlc
    }

    /// Splits the change into parts of at most `budget` wire bytes each.
    pub fn split(self, budget: usize) -> Vec<Change> {
        if self.wire_size() <= budget {
            return vec![self];
        }
        let room = budget
            .saturating_sub(self.wire_size() - self.value.len())
            .max(4);
        let mut parts = Vec::new();
        let mut rest = self.value.as_str();
        while !rest.is_empty() {
            let mut end = room.min(rest.len());
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            let (part, tail) = rest.split_at(end);
            parts.push(Change {
                value: part.to_string(),
                continues: !tail.is_empty(),
                ..self.clone()
            });
            rest = tail;
        }
        parts
    }
}

/// Joins the parts of split values back into one change per cell. Fails
/// when a part chain is broken: a `continues` part followed by another cell
/// or by nothing.
pub fn join_parts(changes: Vec<Change>) -> Result<Vec<Change>, ChangeError> {
    let mut joined: Vec<Change> = Vec::with_capacity(changes.len());
    let mut open = false;
    for change in changes {
        if open {
            let last = joined.last_mut().ok_or(ChangeError::Value)?;
            if !last.same_cell(&change) {
                return Err(ChangeError::Value);
            }
            last.value.push_str(&change.value);
            last.continues = change.continues;
        } else {
            joined.push(change);
        }
        open = joined.last().is_some_and(|last| last.continues);
    }
    if open {
        return Err(ChangeError::Value);
    }
    Ok(joined)
}

/// The size of a transaction group by haex-crdt's canonical rule, the one
/// `Database::write` checks against `max_transaction_bytes`.
pub fn group_bytes(changes: &[ColumnChange]) -> Result<usize, ChangeError> {
    let values = changes
        .iter()
        .map(|change| {
            ValueConverter::json_to_rusqlite_value(&change.value).map_err(|_| ChangeError::SqlValue)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let params: Vec<&dyn ToSql> = values.iter().map(|value| value as &dyn ToSql).collect();
    serialized_parameter_bytes(&params).map_err(|_| ChangeError::SqlValue)
}

#[cfg(test)]
#[path = "change_tests.rs"]
mod tests;

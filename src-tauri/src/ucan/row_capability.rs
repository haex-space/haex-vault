//! Row-level capability wrapper — pairs a row operation (`row_insert`,
//! `row_update`, `row_delete`) with a [`Predicate`] that constrains which
//! rows the audience may operate on.
//!
//! `RowCapability` is the leaf type inside a UCAN payload's row-cap list
//! (see the C.5 envelope work). The parent chain-walker verifies that a
//! child's `RowCapability` is present in the parent's set (attenuation);
//! the row-sig verifier evaluates the predicate against a candidate row
//! payload to decide acceptance.
//!
//! Design notes:
//! - `#[serde(tag = "op")]`, NOT `untagged`: the three variants have
//!   identical field shapes, so a discriminator is mandatory — the
//!   `untagged` fallback would collapse them and lose the operation.
//! - `#[serde(deny_unknown_fields)]`: an authorisation grammar must be
//!   fail-closed. Same rationale as [`crate::ucan::predicate::Predicate`];
//!   see the PR-#761 CodeRabbit finding on `untagged` fail-open.
//! - `where` is a Rust reserved word, so the payload key `where` is
//!   remapped to the field `matches` via `#[serde(rename = "where")]`.

use crate::ucan::predicate::Predicate;
use serde::{Deserialize, Serialize};

/// A row-level UCAN capability: an operation plus a predicate on the row
/// payload. `Vec<RowCapability>` lives inside a UCAN payload, keyed by
/// space id (see C.5 envelope).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op", deny_unknown_fields)]
pub enum RowCapability {
    RowInsert {
        #[serde(rename = "where")]
        matches: Predicate,
    },
    RowUpdate {
        #[serde(rename = "where")]
        matches: Predicate,
    },
    RowDelete {
        #[serde(rename = "where")]
        matches: Predicate,
    },
}

impl RowCapability {
    /// The predicate constraining which rows this capability permits.
    pub fn matches(&self) -> &Predicate {
        match self {
            RowCapability::RowInsert { matches }
            | RowCapability::RowUpdate { matches }
            | RowCapability::RowDelete { matches } => matches,
        }
    }
}

#[cfg(test)]
mod tests;

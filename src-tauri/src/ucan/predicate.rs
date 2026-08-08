//! Predicate DSL for row-level UCAN capabilities.
//!
//! A [`Predicate`] is a small boolean grammar over a row's column values.
//! It exists so a row-cap issuer can say "the audience may write rows
//! matching *this shape*" without needing a full SQL expression evaluator
//! at the puller. The evaluator against a live row payload lives in the
//! same module (C.4); this file only defines the grammar + serde.
//!
//! Grammar:
//! ```text
//! Predicate  := AND | OR | NOT | Eq | In | StartsWith
//! AND        := {"and":[Predicate, ...]}
//! OR         := {"or": [Predicate, ...]}
//! NOT        := {"not": Predicate}
//! Eq         := {"col": string, "eq": Primitive}
//! In         := {"col": string, "in": [Primitive, ...]}
//! StartsWith := {"col": string, "starts_with": string}
//! Primitive  := string | number | bool | null   // objects/arrays rejected
//! ```
//!
//! Design decisions:
//! - **`serde(untagged)`** — no discriminator field; the JSON shape alone
//!   selects the variant. An unknown operator matches no variant and is
//!   rejected at parse time.
//! - **[`PrimitiveValue`]** restricts operands to JSON scalars. Rows are
//!   flat column values in SQLite; comparing against nested JSON would
//!   need semantics we don't want to invent.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A boolean predicate over a row's payload. See module docs for grammar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Predicate {
    And {
        and: Vec<Predicate>,
    },
    Or {
        or: Vec<Predicate>,
    },
    Not {
        not: Box<Predicate>,
    },
    Eq {
        col: String,
        eq: PrimitiveValue,
    },
    In {
        col: String,
        #[serde(rename = "in")]
        values: Vec<PrimitiveValue>,
    },
    StartsWith {
        col: String,
        starts_with: String,
    },
}

/// Restricted operand type for [`Predicate`]. Only JSON scalars — objects
/// and arrays are rejected at parse time so the evaluator can compare
/// against a row's column values without recursive semantics.
///
/// `#[serde(untagged)]` on the enum means: input must match exactly one
/// scalar variant. A JSON object or array reaches this deserializer and
/// is rejected because none of the variants accept those shapes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PrimitiveValue {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
}

// ---------------------------------------------------------------------------
// Row view + evaluator (C.4)
// ---------------------------------------------------------------------------

/// A read-only view of a row's column values, keyed by column name.
///
/// The evaluator only needs point-lookup by column name; anything else
/// (SQLite rows, `HashMap`s, mocks) can implement this trait.
///
/// Semantics for missing columns: [`Self::get`] returns `None`, and the
/// evaluator treats that as [`PrimitiveValue::Null`]. This mirrors
/// SQL's "unknown column" being a NULL value rather than a type error —
/// grants written for a schema variant that a puller doesn't yet have
/// don't blow up, they just never match.
pub trait RowView {
    fn get(&self, col: &str) -> Option<&PrimitiveValue>;
}

impl RowView for HashMap<String, PrimitiveValue> {
    fn get(&self, col: &str) -> Option<&PrimitiveValue> {
        HashMap::get(self, col)
    }
}

impl RowView for HashMap<&str, PrimitiveValue> {
    fn get(&self, col: &str) -> Option<&PrimitiveValue> {
        HashMap::get(self, col)
    }
}

impl Predicate {
    /// Evaluate this predicate against `row`.
    ///
    /// - Missing column ↔ [`PrimitiveValue::Null`] (see [`RowView`] docs).
    /// - Type-mismatched leaf comparisons (e.g. `StartsWith` against a
    ///   non-string column) return `false`, never a panic.
    /// - No side effects, no I/O.
    pub fn eval<R: RowView + ?Sized>(&self, row: &R) -> bool {
        match self {
            Predicate::And { and } => and.iter().all(|p| p.eval(row)),
            Predicate::Or { or } => or.iter().any(|p| p.eval(row)),
            Predicate::Not { not } => !not.eval(row),
            Predicate::Eq { col, eq } => {
                let value = row.get(col).cloned().unwrap_or(PrimitiveValue::Null);
                value == *eq
            }
            Predicate::In { col, values } => {
                let value = row.get(col).cloned().unwrap_or(PrimitiveValue::Null);
                values.contains(&value)
            }
            Predicate::StartsWith { col, starts_with } => match row.get(col) {
                Some(PrimitiveValue::String(s)) => s.starts_with(starts_with.as_str()),
                _ => false,
            },
        }
    }
}

#[cfg(test)]
mod tests;

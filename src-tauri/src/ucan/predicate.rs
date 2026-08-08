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

#[cfg(test)]
mod tests;

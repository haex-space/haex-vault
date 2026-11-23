//! The Abacus AI delegate: the vault's copy of `abacusai` driven in its JSON-lines mode, one
//! process per request (spec 096 FR-007 to FR-010, `data-model.md` §4).
//!
//! Until the adapter of spec 096 (T020) lands, a request ends with a typed "not implemented
//! yet" error and nothing is started.

use crate::adapters::AdapterError;

/// The reason an Abacus request ends with until the adapter exists.
pub(super) fn not_implemented() -> AdapterError {
    AdapterError::Unavailable {
        reason: "abacusai: the adapter is not implemented yet".into(),
    }
}

#[cfg(test)]
#[path = "abacus_tests.rs"]
mod tests;

//! Comprehensive SQL injection prevention tests
//!
//! These tests ensure that the database layer properly prevents various
//! SQL injection attack vectors. The original 700+ LoC monolith has been
//! split into per-vector submodules so each attack class lives next to its
//! own helpers. Test names are identical to the pre-split file so any
//! historical reference (CI logs, prior PRs) still resolves.

#![cfg(test)]

mod comment_smuggling;
mod dangerous_statements;
mod edge_cases;
mod extraction;
mod helpers;
mod multi_statement;
mod string_escape;
mod table_prefix;
mod union_select;

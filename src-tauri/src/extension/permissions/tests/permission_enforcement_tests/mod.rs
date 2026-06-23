//! Comprehensive permission enforcement tests
//!
//! These tests ensure that extensions cannot access resources without proper
//! permissions. The original 650-LoC monolith has been split into per-resource
//! submodules so each enforcement family lives next to its own setup. Test
//! names are identical to the pre-split file so any historical reference
//! (CI logs, prior PRs) still resolves.

#![cfg(test)]

mod combinations;
mod cross_extension;
mod database;
mod edge_cases;
mod helpers;
mod resource_types;
mod status;
mod system_tables;
mod target_patterns;

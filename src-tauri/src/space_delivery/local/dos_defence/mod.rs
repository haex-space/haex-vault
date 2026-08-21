//! DoS-defence layer for the Leader.
//!
//! See `docs/plans/2026-06-13-leader-reject-rate-limit.md`.
//!
//! Phase 1 module skeleton extended with Phase 3 modules
//! (flood-mode state machine + contacts resolver).

pub mod config;
pub mod contacts;
pub mod decision;
pub mod flood_mode;
pub mod handler_rate_gate;
pub mod notifier;
pub mod state;
pub mod tracker;

#[cfg(test)]
mod config_tests;
#[cfg(test)]
mod contacts_tests;
#[cfg(test)]
mod decision_tests;
#[cfg(test)]
mod flood_mode_tests;
#[cfg(test)]
mod handler_rate_gate_tests;
#[cfg(test)]
mod notifier_tests;
#[cfg(test)]
mod tracker_tests;

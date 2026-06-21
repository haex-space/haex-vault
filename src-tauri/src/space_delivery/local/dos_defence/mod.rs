//! DoS-defence layer for the Leader.
//!
//! See `docs/plans/2026-06-13-leader-reject-rate-limit.md`.
//!
//! Phase 1 module skeleton — currently exposes [`tracker`].

pub mod config;
pub mod decision;
pub mod notifier;
pub mod tracker;

#[cfg(test)]
mod config_tests;
#[cfg(test)]
mod decision_tests;
#[cfg(test)]
mod notifier_tests;
#[cfg(test)]
mod tracker_tests;

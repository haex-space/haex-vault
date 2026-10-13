//! The bridge between extension frames and holzi (ADR-0004 direction B, research R14): frame
//! sessions, the allowlist of bridge methods and events to frames.

pub mod blocking;
pub mod database;
pub mod dispatch;
pub mod events;
pub mod frames;
pub(crate) mod agent_tasks;
pub mod methods;
pub mod permissions;

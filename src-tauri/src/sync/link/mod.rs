//! Linking a new installation to a vault (spec 024, user story 5,
//! research R11, contracts/sync-protocol.md `holzi-link/1`).

pub mod code;
pub mod error;
pub mod host;
pub mod host_task;
pub mod join;
pub mod join_task;
pub mod meeting;
pub mod pending;
pub mod status;
pub mod text;
pub mod wire;

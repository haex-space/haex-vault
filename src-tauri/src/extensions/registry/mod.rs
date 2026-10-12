//! The registry of installed extensions: installing, the effective bundle per extension, removing
//! and the life cycle per device (research R11).

pub mod blobs;
mod declared_view;
pub mod effective;
pub mod install;
pub mod lifecycle;
pub mod limits;
pub mod list;
pub mod purge;
pub mod remove;
pub mod start;
pub mod status;

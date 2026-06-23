//! Server-side request handlers for incoming peer storage connections.
//!
//! Each `Request::*` variant has its own submodule. The stream-level
//! UCAN-validation / dispatch lives in [`dispatch`]; per-request handlers
//! live in sibling modules named after the request variant.

mod common;
mod create_directory;
mod delete;
mod dispatch;
mod list;
mod manifest;
mod read;
mod stat;
mod write;

#[cfg(target_os = "android")]
pub(super) use common::send_response_and_finish;
pub(super) use dispatch::handle_stream;

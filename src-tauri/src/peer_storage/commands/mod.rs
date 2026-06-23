//! Tauri commands for peer storage
//!
//! ## Mutex poisoning in `last_emit` throttle locks
//!
//! The per-download progress callbacks (around lines 520-650 / 640-770) use
//! `Mutex<Instant>` locks with `unwrap_or_else(|e| e.into_inner())`. These
//! are throttling timestamps — a poison means at worst one extra progress
//! event slips through before throttling resumes. No data is at risk and no
//! CRDT path is involved, so a critical-failure banner would be misleading.
//! The HLC lock at the top of `peer_storage_start` DOES use `lock_or_fail`.

mod helpers;
mod lifecycle;
mod open_file;
mod remote_fs;
mod transfers;

pub(crate) use helpers::*;
pub use lifecycle::*;
pub use open_file::*;
pub use remote_fs::*;
pub use transfers::*;

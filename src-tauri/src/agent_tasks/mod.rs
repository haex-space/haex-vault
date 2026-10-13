//! Structured, extension-scoped agent tasks.
//!
//! This module is intentionally separate from the free-form chat runtime. Tasks
//! accept a versioned input, resolve a host-owned profile, enforce capabilities,
//! and emit only schema-validated results. Provider credentials and general
//! agent tools never cross the extension boundary.

/// First structured task exposed to extensions.
pub const PAPER_NOTE_RECOGNITION_TASK: &str = "paper-note-recognition";

/// Current wire-schema version for structured agent tasks.
pub const TASK_SCHEMA_VERSION: u32 = 1;

pub(crate) mod bridge;
mod definitions;
mod errors;
mod fixture;
mod model;
mod state;
mod validation;

#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod validation_tests;

pub use errors::{TaskError, TaskErrorCode};
pub(crate) use fixture::run_fixture;
pub use model::{
    PaperNoteInput, PaperNoteOptions, PaperNoteRegion, PaperNoteRegionKind, PaperNoteResult,
    ProcessingMode, TaskInput, TaskProvenance,
};
pub(crate) use state::{TaskId, TaskOwner, TaskRegistry, TaskRunStatus};
pub(crate) use validation::validate_paper_note_result;

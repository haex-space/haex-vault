//! Model jobs of extensions (spec 055; the structured agent tasks of spec 047 are being
//! generalised into them). The parts of the job format are new and not yet wired into the bridge:
//! `job`, `schema`, `guard`, `effort`, `place`, `limits`. The task lifecycle below is the one of 047.
//!
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
#[allow(
    dead_code,
    reason = "used by the job runner of the next step of delivery B"
)]
pub(crate) mod effort;
mod errors;
mod fixture;
#[allow(
    dead_code,
    reason = "used by the job runner of the next step of delivery B"
)]
mod guard;
#[allow(
    dead_code,
    reason = "used by the job runner of the next step of delivery B"
)]
mod job;
#[allow(
    dead_code,
    reason = "used by the job runner of the next step of delivery B"
)]
mod limits;
mod model;
#[allow(
    dead_code,
    reason = "used by the job runner of the next step of delivery B"
)]
mod place;
#[allow(
    dead_code,
    reason = "used by the job runner of the next step of delivery B"
)]
mod schema;
mod state;
mod validation;

#[cfg(test)]
mod effort_tests;
#[cfg(test)]
mod errors_tests;
#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod job_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod place_tests;
#[cfg(test)]
mod schema_tests;
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

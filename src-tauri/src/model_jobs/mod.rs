//! Model jobs of extensions (spec 055; the structured agent tasks of spec 047 are generalised
//! into them). An extension sends a job of one to four steps, each with an instruction, an input
//! and the format of its answer; holzi picks the models, checks what they answer and gives the
//! extension only a result that fits its format.
//!
//! What exists so far: the parts of a job (`job`, `schema`, `guard`, `effort`, `place`,
//! `limits`), the errors (`errors`), the registry of running jobs (`state`) and the bridge
//! methods with their preflight (`bridge`). The run of a job, the candidates of a step and the
//! list of models follow with the stories.

// The parts below are used by the runner of the first story; until then only the bridge's
// preflight and the registry's cancel reach them.
#![allow(
    dead_code,
    reason = "used by the job runner of the next step of delivery B"
)]

pub(crate) mod bridge;
pub(crate) mod effort;
mod errors;
mod guard;
mod job;
mod limits;
mod place;
mod schema;
mod state;

#[cfg(test)]
mod effort_tests;
#[cfg(test)]
mod errors_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod job_tests;
#[cfg(test)]
mod place_tests;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod state_tests;

pub(crate) use state::{JobOwner, JobRegistry};

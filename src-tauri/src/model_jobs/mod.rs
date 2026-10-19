//! Model jobs of extensions (spec 055; the structured agent tasks of spec 047 are generalised
//! into them). An extension sends a job of one to four steps, each with an instruction, an input
//! and the format of its answer; holzi picks the models, checks what they answer and gives the
//! extension only a result that fits its format.
//!
//! A job of one step runs: the parts of a job (`job`, `schema`, `guard`, `effort`, `place`,
//! `limits`), the errors (`errors`), the registry of running jobs (`state`), the models a step
//! may go to (`candidates`, `model_source`), the preflight (`preflight`), the run (`runner`) and
//! the bridge methods (`bridge`). Jobs of several steps follow with user story 3.

// The parts for several steps (`from`) and the registry's `expire` are not reached yet.
#![allow(
    dead_code,
    reason = "the steps of user story 3 use the rest of the job parts"
)]

pub(crate) mod bridge;
mod candidates;
pub(crate) mod effort;
mod errors;
mod guard;
mod job;
mod limits;
pub(crate) mod model_source;
mod place;
mod preflight;
mod runner;
mod schema;
mod state;
#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod candidates_tests;
#[cfg(test)]
mod effort_tests;
#[cfg(test)]
mod errors_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod job_tests;
#[cfg(test)]
mod model_source_tests;
#[cfg(test)]
mod place_tests;
#[cfg(test)]
mod runner_tests;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod state_tests;

pub(crate) use state::{JobOwner, JobRegistry};

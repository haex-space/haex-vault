//! The bridge between extension frames and holzi (ADR-0004 direction B, research R14): frame
//! sessions, the allowlist of bridge methods and events to frames.

mod allowlist;
pub mod blocking;
pub mod database;
pub mod dispatch;
pub mod events;
pub mod frames;
pub mod methods;
pub(crate) mod model_jobs;
pub mod permissions;

#[cfg(test)]
mod model_jobs_capabilities_tests;
#[cfg(test)]
mod model_jobs_consent_tests;
#[cfg(test)]
mod model_jobs_page_tests;
#[cfg(test)]
mod model_jobs_run_tests;
#[cfg(test)]
mod model_jobs_steps_tests;
#[cfg(test)]
mod model_jobs_tests;

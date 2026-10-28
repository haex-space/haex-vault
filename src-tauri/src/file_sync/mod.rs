//! Sync rules (spec 061): rules that keep two places of the file browser equal, one-way or both ways,
//! on the device that created them. A run lists both places, compares them with the state of the
//! last success and builds a plan before it changes anything (research R3).

pub mod commands;
pub mod conflict;
pub mod execute;
pub mod ignore;
pub mod model;
pub mod place;
pub mod place_device;
pub mod place_storage;
pub mod plan;
pub mod progress;
pub mod rules;
pub mod run;
pub mod scan;
pub mod schedule;
pub mod service;
pub mod status;
pub mod store;
#[cfg(test)]
pub mod test_support;
pub mod watch;

//! Sync rules (spec 061): rules that keep two places of the file browser equal, one-way or both ways,
//! on the device that created them. A run lists both places, compares them with the state of the
//! last success and builds a plan before it changes anything (research R3).

pub mod model;
pub mod rules;
pub mod store;

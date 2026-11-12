//! Passes: cards and tickets, the second entry type of the password manager (spec 092). The codes
//! module checks, draws and reads codes; the model holds the input types; the store writes a pass in
//! the caller's transaction.

pub mod codes;
#[cfg(test)]
mod codes_tests;
pub mod model;
#[cfg(test)]
mod pass_tests;
pub mod store;

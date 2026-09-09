mod conflicts;
mod db;
mod delete_propagation;
mod finish;
mod grouping;
mod policy;
mod registry_row_gate;
mod schema_recovery;
mod signatures;
mod types;

#[cfg(feature = "e2e-hooks")]
pub mod e2e_hooks;

#[cfg(all(test, feature = "e2e-hooks"))]
mod e2e_hooks_tests;

pub use db::*;
pub(crate) use grouping::*;
pub use types::*;

#[cfg(test)]
#[path = "../../commands_pending_columns_tests.rs"]
mod pending_columns_tests;

#[cfg(test)]
#[path = "../../commands_delete_resurrection_tests.rs"]
mod delete_resurrection_tests;

#[cfg(test)]
#[path = "../../commands_apply_registry_row_sig_tests.rs"]
mod apply_registry_row_sig_tests;

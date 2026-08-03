mod db;
mod delete_propagation;
mod grouping;
mod registry_row_gate;
mod types;

#[cfg(feature = "e2e-hooks")]
pub mod e2e_hooks;

#[cfg(all(test, feature = "e2e-hooks"))]
mod e2e_hooks_tests;

pub use db::*;
pub(crate) use grouping::*;
pub use types::*;

// Bring the private delete-propagation helpers into this module's scope so the
// #[path]-included test files (children of this module) can reach them as
// `super::insert_suppressed_by_deletes` etc. — matching the way they were
// referenced before apply.rs became a directory.
#[cfg(test)]
#[allow(unused_imports)]
use delete_propagation::{delete_shadows_insert, insert_suppressed_by_deletes};

#[cfg(test)]
#[path = "../../commands_pending_columns_tests.rs"]
mod pending_columns_tests;

#[cfg(test)]
#[path = "../../commands_delete_resurrection_tests.rs"]
mod delete_resurrection_tests;

#[cfg(test)]
#[path = "../../commands_apply_registry_row_sig_tests.rs"]
mod apply_registry_row_sig_tests;

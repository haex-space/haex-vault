//! Permissions of extensions (spec 017, contracts/permissions.md, research R15): what a
//! permission is, how a request matches it, and the decision. Pure — no database — so every rule
//! is testable on its own; storage and prompts build on it.

mod evaluate;
pub mod manifest_map;
mod model;
pub mod prompts;
pub mod store;
mod target;

pub use evaluate::{evaluate, Decision};
pub use model::{
    Action, GrantScope, Permission, PermissionKind, PermissionRequest, PermissionStatus, VAULT_WIDE,
};
pub use target::{FolderRef, HostPattern, RequestTarget, Target, UrlPattern, WebRequest};

#[cfg(test)]
#[path = "evaluate_tests.rs"]
mod evaluate_tests;
#[cfg(test)]
#[path = "target_tests.rs"]
mod target_tests;

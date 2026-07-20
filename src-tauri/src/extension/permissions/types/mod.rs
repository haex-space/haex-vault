//! Permission type definitions split across cohesive submodules:
//! - [`principal`] — the [`Principal`] enum (the actor a check runs against).
//! - [`actions`] — per-resource action enums (`DbAction`, `FsAction`, ...) and
//!   the umbrella [`Action`] container.
//! - [`constraints`] — typed constraint structs, the [`PermissionConstraints`]
//!   enum, and the `split_*` / `combine_constraints` helpers.
//! - [`permission`] — [`ExtensionPermission`], [`ResourceType`],
//!   [`PermissionStatus`] and the conversions to/from the DB row.
//!
//! Every previously-public symbol is re-exported here so existing
//! `crate::extension::permissions::types::Symbol` import paths keep working.

mod actions;
mod constraints;
mod permission;
mod principal;

pub use actions::{
    Action, BookmarksAction, DbAction, ExtensionApiAction, FsAction, IdentityAction, MailAction,
    NotificationsAction, PasswordsAction, PasswordsScope, RwAction, ShellAction, SpaceAction,
    WebAction,
};
// `DbConstraints`/`FsConstraints`/etc. and `combine_constraints` are reached
// only from tests via this path; the lib build flags the re-exports as
// "unused" even though they're part of the module's public surface.
#[allow(unused_imports)]
pub(crate) use constraints::{combine_constraints, split_constraints, split_constraints_value};
#[allow(unused_imports)]
pub use constraints::{
    DbConstraints, FsConstraints, PermissionConstraints, RateLimit, ShellConstraints,
    WebConstraints,
};
pub use permission::{ExtensionPermission, PermissionStatus, ResourceType};
pub use principal::Principal;

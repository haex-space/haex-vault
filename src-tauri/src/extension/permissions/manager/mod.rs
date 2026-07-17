pub(super) mod check;
mod crud;
mod path;
mod url;

pub struct PermissionManager;

// Re-export the deny-first resolver for sibling extension modules
// (e.g. `extension::sync_tables`) that need to resolve a set of matching
// permission statuses without reaching through the `pub(super)` `check`
// module boundary.
pub(crate) use check::deny_first_precedence;

// `session.rs` (a sibling of `manager` under `permissions`) needs this to
// implement the session-store counterpart of `PermissionManager::save_passwords_grant`
// with the exact same tag-cleaning / default-label validation.
pub(crate) use check::passwords::normalize_passwords_grant_tags;

#[cfg(test)]
pub(crate) use check::database::database_matching_status;
#[cfg(test)]
pub(crate) use check::filesystem::{
    filesystem_matching_has_constraint_violation, filesystem_matching_status,
    format_filesystem_denied_target,
};
#[cfg(test)]
pub(crate) use check::identities::{
    identities_matching_status, resolve_identities_decision, IdentitiesDecision,
};
#[cfg(test)]
pub(crate) use check::passwords::{
    parse_passwords_default_marker, passwords_session_scope, resolve_passwords_tags_scope,
    PasswordsGrantRow,
};
#[cfg(test)]
pub(crate) use check::rw_resource::{rw_resource_matching_status, rw_resource_session_status};
#[cfg(test)]
pub(crate) use check::shell::{
    format_shell_denied_target, shell_matching_has_constraint_violation, shell_matching_status,
};
#[cfg(test)]
pub(crate) use check::spaces::{spaces_matching_status, spaces_session_status};
#[cfg(test)]
pub(crate) use check::web::web_matching_status;

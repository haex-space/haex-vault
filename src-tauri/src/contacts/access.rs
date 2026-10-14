//! The access rules of the address book (spec 051 FR-028 to FR-031, research R1, R2).
//!
//! Pure: no database. A caller and its grants go in, the decision comes out; the service
//! ([`super::service`]) calls these before every method. The user's window and holzi's own
//! functions need no grant (FR-028); every agent, the built-in one too, and every extension only
//! reach what a grant covers (FR-029, FR-030). A contact outside the scope is `NotFound`, the same
//! answer as for a missing one (FR-031); a missing grant of the asked kind is `Forbidden`.

use std::collections::BTreeSet;

use crate::error::HolziError;

pub use crate::caller::Caller;

/// `ReadWrite` covers `Read`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantAction {
    Read,
    ReadWrite,
}

/// Which contacts a grant covers: all, or those in one of the groups (by group id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    Groups(BTreeSet<String>),
}

impl Scope {
    fn union(self, other: Scope) -> Scope {
        match (self, other) {
            (Scope::All, _) | (_, Scope::All) => Scope::All,
            (Scope::Groups(mut a), Scope::Groups(b)) => {
                a.extend(b);
                Scope::Groups(a)
            }
        }
    }

    /// Whether a contact in these groups lies in this scope.
    pub fn covers(&self, groups: &[String]) -> bool {
        match self {
            Scope::All => true,
            Scope::Groups(scope) => groups.iter().any(|group| scope.contains(group)),
        }
    }
}

/// What a caller may do with which contacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub action: GrantAction,
    pub scope: Scope,
}

impl Grant {
    pub fn new(action: GrantAction, scope: Scope) -> Self {
        Self { action, scope }
    }
}

/// Why a request is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    /// No grant of the asked kind, or a method only the user may call.
    Forbidden,
    /// The contact is outside the scope or missing; the two cannot be told apart (FR-031).
    NotFound,
}

impl From<Denied> for HolziError {
    fn from(denied: Denied) -> Self {
        match denied {
            Denied::Forbidden => HolziError::ContactsForbidden,
            Denied::NotFound => HolziError::ContactsNotFound,
        }
    }
}

/// The contacts a caller reaches with grants of the asked kind: the user and holzi's own functions
/// all of them, everybody else the union of the scopes of the grants that cover the kind.
fn reach(caller: &Caller, grants: &[Grant], needed: GrantAction) -> Result<Scope, Denied> {
    match caller {
        Caller::User | Caller::Internal { .. } => Ok(Scope::All),
        Caller::BuiltinAgent | Caller::Extension { .. } | Caller::ExternalAgent { .. } => grants
            .iter()
            .filter(|grant| grant.action == GrantAction::ReadWrite || needed == GrantAction::Read)
            .map(|grant| grant.scope.clone())
            .reduce(Scope::union)
            .ok_or(Denied::Forbidden),
    }
}

/// Methods that only the window offers (groups, photos of the own window, settings).
pub fn require_user(caller: &Caller) -> Result<(), Denied> {
    match caller {
        Caller::User => Ok(()),
        _ => Err(Denied::Forbidden),
    }
}

/// The scope of a list for this caller.
pub fn authorize_list(caller: &Caller, grants: &[Grant]) -> Result<Scope, Denied> {
    reach(caller, grants, GrantAction::Read)
}

/// Reading one contact in these groups.
pub fn authorize_read(caller: &Caller, grants: &[Grant], groups: &[String]) -> Result<(), Denied> {
    in_scope(&reach(caller, grants, GrantAction::Read)?, groups)
}

/// Changing or deleting one contact in these groups.
pub fn authorize_write(caller: &Caller, grants: &[Grant], groups: &[String]) -> Result<(), Denied> {
    in_scope(&reach(caller, grants, GrantAction::ReadWrite)?, groups)
}

/// Creating a contact in these groups: with a group scope the new contact must lie in it.
pub fn authorize_create(
    caller: &Caller,
    grants: &[Grant],
    groups: &[String],
) -> Result<(), Denied> {
    if reach(caller, grants, GrantAction::ReadWrite)?.covers(groups) {
        Ok(())
    } else {
        Err(Denied::Forbidden)
    }
}

fn in_scope(scope: &Scope, groups: &[String]) -> Result<(), Denied> {
    if scope.covers(groups) {
        Ok(())
    } else {
        Err(Denied::NotFound)
    }
}

#[cfg(test)]
#[path = "access_tests.rs"]
mod tests;

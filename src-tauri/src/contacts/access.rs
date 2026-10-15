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

/// Which contacts a grant covers: all, those in one of the groups, or all but those in a denied
/// group (group ids throughout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    Groups(BTreeSet<String>),
    /// Every contact except one in a group of `denied`; a contact also in a group of `granted` is
    /// covered all the same, because a granted group beats a denied one (spec 017 FR-017, as for
    /// the tags of the password manager).
    AllExcept {
        denied: BTreeSet<String>,
        granted: BTreeSet<String>,
    },
}

impl Scope {
    /// Every contact but those in `denied`; without a denied group simply all.
    pub fn all_except(denied: BTreeSet<String>) -> Scope {
        if denied.is_empty() {
            Scope::All
        } else {
            Scope::AllExcept {
                denied,
                granted: BTreeSet::new(),
            }
        }
    }

    // Two `AllExcept` scopes join with the union of their denied groups, which hides more than the
    // exact union; all grants of one caller carry the same denials, so it is exact there.
    fn union(self, other: Scope) -> Scope {
        match (self, other) {
            (Scope::All, _) | (_, Scope::All) => Scope::All,
            (Scope::Groups(mut a), Scope::Groups(b)) => {
                a.extend(b);
                Scope::Groups(a)
            }
            (
                Scope::AllExcept {
                    denied,
                    mut granted,
                },
                Scope::Groups(groups),
            )
            | (
                Scope::Groups(groups),
                Scope::AllExcept {
                    denied,
                    mut granted,
                },
            ) => {
                granted.extend(groups);
                Scope::AllExcept { denied, granted }
            }
            (
                Scope::AllExcept {
                    mut denied,
                    mut granted,
                },
                Scope::AllExcept {
                    denied: d,
                    granted: g,
                },
            ) => {
                denied.extend(d);
                granted.extend(g);
                Scope::AllExcept { denied, granted }
            }
        }
    }

    /// Whether a contact in these groups lies in this scope.
    pub fn covers(&self, groups: &[String]) -> bool {
        let has = |set: &BTreeSet<String>| groups.iter().any(|group| set.contains(group));
        match self {
            Scope::All => true,
            Scope::Groups(scope) => has(scope),
            Scope::AllExcept { denied, granted } => has(granted) || !has(denied),
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

/// Methods for the window and holzi's own functions, such as reading the groups to turn the
/// group names of an extension's permissions into grants.
pub fn require_holzi(caller: &Caller) -> Result<(), Denied> {
    match caller {
        Caller::User | Caller::Internal { .. } => Ok(()),
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

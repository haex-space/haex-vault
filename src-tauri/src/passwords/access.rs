//! The access rules of the password manager (spec 034, `contracts/access.md`, rules Z1–Z13).
//!
//! Pure: no database, no frontend. A caller (the window, the built-in agent, an extension, an
//! external agent, a holzi function) and the caller's grants go in, the decision comes out. The
//! service ([`super::service`]) calls these functions before every method; where grants are stored,
//! issued, shown and revoked is the business of the specs 017–019 and 021, not of this module.
//!
//! An item outside the scope is `NotFound`, the same answer as for a missing one (FR-029); a missing
//! grant of the asked kind is `Forbidden` and is checked first (Z3). Tag names compare through
//! [`ids::fold`].

use std::collections::BTreeSet;

use super::ids::fold;
use crate::error::HolziError;

pub use crate::caller::Caller;

/// `ReadWrite` covers `Read`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantAction {
    Read,
    ReadWrite,
}

/// Which entries a grant covers: all of them, those that carry one of the (folded) tags, or all
/// but those that carry a denied tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    Tags(BTreeSet<String>),
    /// Every entry except one that carries a tag of `denied`; an entry that also carries a tag of
    /// `granted` is covered all the same, because a granted tag beats a denied one (spec 017,
    /// FR-017 for passwords).
    AllExcept {
        denied: BTreeSet<String>,
        granted: BTreeSet<String>,
    },
}

impl Scope {
    /// A scope from tag names; a `*` anywhere makes it [`Scope::All`].
    pub fn tags<I, S>(names: I) -> Scope
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut folded = BTreeSet::new();
        for name in names {
            if name.as_ref().trim() == "*" {
                return Scope::All;
            }
            folded.insert(fold(name.as_ref()));
        }
        Scope::Tags(folded)
    }

    /// All entries but those carrying one of the denied tag names; none denied is [`Scope::All`].
    pub fn all_except<I, S>(denied: I) -> Scope
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let denied: BTreeSet<String> = denied.into_iter().map(|n| fold(n.as_ref())).collect();
        if denied.is_empty() {
            return Scope::All;
        }
        Scope::AllExcept {
            denied,
            granted: BTreeSet::new(),
        }
    }

    // ponytail: two `AllExcept` scopes join with the union of their denied tags, which hides more
    // than the exact union (ceiling: grants with different denials; holzi gives all grants of one
    // caller the same ones, so the result is exact there).
    fn union(self, other: Scope) -> Scope {
        match (self, other) {
            (Scope::All, _) | (_, Scope::All) => Scope::All,
            (Scope::Tags(mut a), Scope::Tags(b)) => {
                a.extend(b);
                Scope::Tags(a)
            }
            (
                Scope::AllExcept {
                    denied,
                    mut granted,
                },
                Scope::Tags(tags),
            )
            | (
                Scope::Tags(tags),
                Scope::AllExcept {
                    denied,
                    mut granted,
                },
            ) => {
                granted.extend(tags);
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

    /// Whether a tag name lies in this scope.
    pub fn contains(&self, name: &str) -> bool {
        match self {
            Scope::All => true,
            Scope::Tags(tags) => tags.contains(&fold(name)),
            Scope::AllExcept { denied, granted } => {
                let name = fold(name);
                granted.contains(&name) || !denied.contains(&name)
            }
        }
    }

    /// Whether an entry with these tags lies in this scope.
    pub fn covers(&self, tags: &[String]) -> bool {
        match self {
            Scope::All => true,
            Scope::Tags(_) => tags.iter().any(|tag| self.contains(tag)),
            Scope::AllExcept { denied, granted } => {
                let has = |set: &BTreeSet<String>| tags.iter().any(|tag| set.contains(&fold(tag)));
                has(granted) || !has(denied)
            }
        }
    }
}

/// What a caller may do with which entries; issued by the specs 017–019 and 021.
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

/// What the rules need to know about a stored entry: its tag names, whether it is in the trash and
/// which holzi function it belongs to (`None`: the user's own, Z14).
#[derive(Debug, Clone, Copy)]
pub struct ItemState<'a> {
    pub tags: &'a [String],
    pub in_trash: bool,
    pub owner: Option<&'a str>,
}

/// Why a request is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    /// No grant of the asked kind (Z3), or a method that only the user may call (Z11).
    Forbidden,
    /// The entry is outside the scope, in the trash for a caller from outside, or missing; the
    /// three cannot be told apart (Z5, Z13).
    NotFound,
}

impl From<Denied> for HolziError {
    fn from(denied: Denied) -> Self {
        match denied {
            Denied::Forbidden => HolziError::PasswordsForbidden,
            Denied::NotFound => HolziError::PasswordsNotFound,
        }
    }
}

/// What `list_headers` delivers to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListView {
    /// The headers of the entries in the scope.
    Items(Scope),
    /// The narrow agent headers (title, tags, folder, TOTP flag) of the built-in agent (Z2).
    Agent,
}

/// What a caller may reach with one kind of grant.
struct Reach {
    scope: Scope,
    /// Only the user sees entries in the trash (Z13).
    sees_trash: bool,
}

/// The reach of the caller for the asked kind: the user everything, the built-in agent nothing
/// (Z2), everybody else the union of the scopes of the grants that cover the kind (Z3).
// ponytail: the scope is recomputed on every call (ceiling: callers with many grants; upgrade
// path: cache the union per caller until its grants change).
fn reach(caller: &Caller, grants: &[Grant], needed: GrantAction) -> Result<Reach, Denied> {
    match caller {
        Caller::User => Ok(Reach {
            scope: Scope::All,
            sees_trash: true,
        }),
        Caller::BuiltinAgent => Err(Denied::Forbidden),
        Caller::Extension { .. }
        | Caller::ExternalAgent { .. }
        | Caller::Internal { .. }
        | Caller::BrowserClient { .. } => grants
            .iter()
            .filter(|grant| covers(grant.action, needed))
            .map(|grant| grant.scope.clone())
            .reduce(Scope::union)
            .map(|scope| Reach {
                scope,
                sees_trash: false,
            })
            .ok_or(Denied::Forbidden),
    }
}

/// `ReadWrite` covers `Read`, nothing else crosses over.
fn covers(granted: GrantAction, needed: GrantAction) -> bool {
    granted == GrantAction::ReadWrite || needed == GrantAction::Read
}

/// Z11: every method beyond the five item methods is for the user alone.
pub fn require_user(caller: &Caller) -> Result<(), Denied> {
    match caller {
        Caller::User => Ok(()),
        _ => Err(Denied::Forbidden),
    }
}

/// `list_headers`: the user and any caller with a read grant see items (the user all of them, also
/// the trash for the user's own overview); the built-in agent gets the agent headers (Z2, Z4).
pub fn authorize_list(caller: &Caller, grants: &[Grant]) -> Result<ListView, Denied> {
    if matches!(caller, Caller::BuiltinAgent) {
        return Ok(ListView::Agent);
    }
    reach(caller, grants, GrantAction::Read).map(|reach| ListView::Items(reach.scope))
}

/// Z14 (spec 038): an entry that belongs to a holzi function exists only for the user and for that
/// function; no grant, not even one for all entries, reaches it.
pub fn sees_owned(caller: &Caller, owner: Option<&str>) -> bool {
    match (owner, caller) {
        (None, _) | (Some(_), Caller::User) => true,
        (Some(owner), Caller::Internal { feature }) => *feature == owner,
        (Some(_), _) => false,
    }
}

/// An entry is visible to the caller when the caller may see its owner (Z14), it is in the scope
/// and, outside the user, not in the trash (Z5, Z13).
fn visible(caller: &Caller, reach: &Reach, state: &ItemState<'_>) -> Result<(), Denied> {
    if !sees_owned(caller, state.owner) {
        return Err(Denied::NotFound);
    }
    if state.in_trash && !reach.sees_trash {
        return Err(Denied::NotFound);
    }
    if !reach.scope.covers(state.tags) {
        return Err(Denied::NotFound);
    }
    Ok(())
}

/// `read_secret_item` (Z3, Z5, Z13).
pub fn authorize_read(
    caller: &Caller,
    grants: &[Grant],
    state: &ItemState<'_>,
) -> Result<(), Denied> {
    visible(caller, &reach(caller, grants, GrantAction::Read)?, state)
}

/// `create_item` (Z6): with a tag scope the submitted tags must all lie in it and there must be at
/// least one; a scope for all but denied tags takes no denied tag.
pub fn authorize_create(
    caller: &Caller,
    grants: &[Grant],
    submitted: &[String],
) -> Result<(), Denied> {
    let reach = reach(caller, grants, GrantAction::ReadWrite)?;
    let needs_a_tag = matches!(reach.scope, Scope::Tags(_));
    let all_in_scope = submitted.iter().all(|tag| reach.scope.contains(tag));
    if all_in_scope && !(needs_a_tag && submitted.is_empty()) {
        Ok(())
    } else {
        Err(Denied::Forbidden)
    }
}

/// `update_item` (Z7, Z12): returns the tag names the entry carries afterwards. Tags outside the
/// scope stay as they are, a tag outside the scope cannot be added, and the entry must keep at
/// least one tag of the scope. `submitted` is `None` when the update does not touch the tags.
pub fn authorize_update(
    caller: &Caller,
    grants: &[Grant],
    state: &ItemState<'_>,
    submitted: Option<&[String]>,
) -> Result<Vec<String>, Denied> {
    let reach = reach(caller, grants, GrantAction::ReadWrite)?;
    visible(caller, &reach, state)?;
    let Some(submitted) = submitted else {
        return Ok(state.tags.to_vec());
    };
    if matches!(reach.scope, Scope::All) {
        return Ok(submitted.to_vec());
    }
    // Sending a tag outside the scope that the entry already carries is not adding it.
    let carries = |name: &str| state.tags.iter().any(|tag| fold(tag) == fold(name));
    if submitted
        .iter()
        .any(|tag| !reach.scope.contains(tag) && !carries(tag))
    {
        return Err(Denied::Forbidden);
    }
    let mut result: Vec<String> = state
        .tags
        .iter()
        .filter(|tag| !reach.scope.contains(tag))
        .cloned()
        .collect();
    let mut seen = BTreeSet::new();
    for tag in submitted.iter().filter(|tag| reach.scope.contains(tag)) {
        if seen.insert(fold(tag)) {
            result.push(tag.clone());
        }
    }
    if reach.scope.covers(&result) {
        Ok(result)
    } else {
        Err(Denied::Forbidden)
    }
}

/// `delete_item` (Z8): moves the entry to the trash; needs a write grant and an entry in the scope.
pub fn authorize_delete(
    caller: &Caller,
    grants: &[Grant],
    state: &ItemState<'_>,
) -> Result<(), Denied> {
    visible(
        caller,
        &reach(caller, grants, GrantAction::ReadWrite)?,
        state,
    )
}

/// A record without an entry (a passkey with no `item_id`) belongs to no tag scope; only a grant
/// for all entries covers it, also one that excepts denied tags (Z9).
pub fn authorize_unassigned(
    caller: &Caller,
    grants: &[Grant],
    needed: GrantAction,
) -> Result<(), Denied> {
    if reach(caller, grants, needed)?.scope.covers(&[]) {
        Ok(())
    } else {
        Err(Denied::NotFound)
    }
}

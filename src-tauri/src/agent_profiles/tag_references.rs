//! Which agent profiles name a tag in their password rights (spec 100, FR-010).
//!
//! The rights of a profile reach entries by tag name, so merging two tags can widen what a profile
//! reaches: the entries that only carried the source then carry the target. The merge dialog asks
//! this before the user confirms. A profile names a tag through its tag list or the tag its new
//! entries get, compared by `fold` like every tag name; a profile that reaches every entry, or has
//! no password rights, names none.

use std::collections::BTreeSet;

use super::store;
use crate::mcp_server::grant_rows::rights_of;
use crate::passwords::ids::fold;
use crate::storage::query::Query;

/// The sorted, unique names of the profiles whose password rights name `tag_name`.
pub fn profiles_naming_tag(q: &mut impl Query, tag_name: &str) -> haex_crdt::Result<Vec<String>> {
    let wanted = fold(tag_name);
    let mut names = BTreeSet::new();
    for profile in store::profiles(q)? {
        let rights = rights_of(store::rights_rows(q, profile.id)?);
        let Some(passwords) = rights.passwords else {
            continue;
        };
        let listed = passwords
            .tags
            .iter()
            .flatten()
            .any(|name| fold(name) == wanted);
        let default = passwords
            .default_tag
            .as_deref()
            .is_some_and(|name| fold(name) == wanted);
        if listed || default {
            names.insert(profile.name);
        }
    }
    Ok(names.into_iter().collect())
}

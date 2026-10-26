//! The name a share carries (spec 060 FR-011): the name parts, else the `npub` of the vault
//! identity, else nothing. Display name, nickname, organization, phone and e-mail are never a
//! substitute; they are shared as details of their own.

use super::envelope::NameParts;
use crate::contacts::card::received;
use crate::contacts::model::Head;

fn present(part: &Option<String>) -> Option<String> {
    part.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// The name of a share of a contact with `head` and the name parts that are there. `vault` is the
/// vault identity of the contact when it has one (the own one, or that of a stored card).
pub fn name_for(head: &Head, vault: Option<[u8; 32]>) -> (String, NameParts) {
    let parts = NameParts {
        prefix: present(&head.name_prefix),
        given: present(&head.given_name),
        middle: present(&head.middle_name),
        family: present(&head.family_name),
        suffix: present(&head.name_suffix),
    };
    let joined = [
        &parts.prefix,
        &parts.given,
        &parts.middle,
        &parts.family,
        &parts.suffix,
    ]
    .into_iter()
    .flatten()
    .cloned()
    .collect::<Vec<_>>()
    .join(" ");
    let name = if !joined.is_empty() {
        joined
    } else {
        vault
            .map(|vault| received::npub(&vault))
            .unwrap_or_default()
    };
    (name, parts)
}

#[cfg(test)]
#[path = "name_tests.rs"]
mod tests;

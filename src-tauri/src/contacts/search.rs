//! The search of the address book (spec 051 FR-004, research R11): a contact matches when its
//! name, name parts, organization, a phone number or an e-mail address contains the query, without
//! regard to case or accents; a query of only phone characters also matches the phone numbers as
//! bare digits, so "01701234567" finds "0170 123-45 67". The window runs the same rule in
//! `src/lib/contacts/search.ts`; both are tested against `src/lib/contacts/search-cases.json`.

use crate::passwords::ids::fold_for_search;

use super::model::ContactHeader;

/// The text of a contact a query is compared with: every searched field folded, one per line, and
/// the phone numbers once more as bare digits.
pub fn haystack(header: &ContactHeader) -> String {
    let mut lines: Vec<String> = [
        Some(header.name.as_str()),
        header.given_name.as_deref(),
        header.family_name.as_deref(),
        header.organization.as_deref(),
    ]
    .into_iter()
    .flatten()
    .chain(header.phones.iter().map(String::as_str))
    .chain(header.emails.iter().map(String::as_str))
    .map(fold_for_search)
    .collect();
    lines.extend(header.phones.iter().map(|phone| digits(phone)));
    lines.join("\n")
}

/// Whether a contact with this haystack matches `query`; an empty query matches everything.
pub fn matches(haystack: &str, query: &str) -> bool {
    let query = fold_for_search(query.trim());
    if query.is_empty() || haystack.contains(&query) {
        return true;
    }
    let bare = digits(&query);
    is_phone_like(&query) && !bare.is_empty() && haystack.contains(&bare)
}

fn digits(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

/// Only digits and the characters phone numbers are written with.
fn is_phone_like(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_ascii_digit() || " +-()./".contains(c))
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

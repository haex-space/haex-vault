//! Matching the phone's contacts with the address book before the first run (spec 051 FR-019,
//! research R8). A clear match is the same name and at least one same number or e-mail address,
//! with exactly one candidate; a match on the name alone, on a number or address alone, or with
//! several candidates is uncertain and goes to the person.

use std::collections::BTreeSet;

use unicode_normalization::UnicodeNormalization;

/// What two contacts are compared by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keys {
    pub names: BTreeSet<String>,
    pub phones: Vec<String>,
    pub emails: BTreeSet<String>,
}

/// The names a contact goes by: the display name and the given and family name in both orders.
pub fn keys(
    display: Option<&str>,
    given: Option<&str>,
    family: Option<&str>,
    phones: &[&str],
    emails: &[&str],
) -> Keys {
    let mut names = BTreeSet::new();
    let given = given.map(name_key).filter(|key| !key.is_empty());
    let family = family.map(name_key).filter(|key| !key.is_empty());
    if let Some(display) = display.map(name_key).filter(|key| !key.is_empty()) {
        names.insert(display);
    }
    match (&given, &family) {
        (Some(given), Some(family)) => {
            names.insert(format!("{given} {family}"));
            names.insert(format!("{family} {given}"));
        }
        (Some(only), None) | (None, Some(only)) => {
            names.insert(only.clone());
        }
        (None, None) => {}
    }
    Keys {
        names,
        phones: phones
            .iter()
            .map(|phone| phone_digits(phone))
            .filter(|digits| digits.trim_start_matches('+').len() >= MIN_PHONE_DIGITS)
            .collect(),
        emails: emails
            .iter()
            .map(|email| email.trim().to_lowercase())
            .filter(|email| !email.is_empty())
            .collect(),
    }
}

/// NFKC, lower case, white space folded to single spaces.
pub fn name_key(name: &str) -> String {
    name.nfkc()
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The digits of a number with a leading `+` kept.
pub fn phone_digits(phone: &str) -> String {
    let trimmed = phone.trim();
    let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
    if trimmed.starts_with('+') {
        format!("+{digits}")
    } else {
        digits
    }
}

/// The shorter of two numbers needs this many digits to match the end of the longer one.
const MIN_PHONE_DIGITS: usize = 7;

/// Two numbers match when the longer ends with the shorter, which has at least seven digits: so
/// `+49 170 1234567` matches `0170 1234567` (the trunk `0` and the country code differ only at
/// the start).
pub fn phones_match(a: &str, b: &str) -> bool {
    let a = a.trim_start_matches('+');
    let b = b.trim_start_matches('+');
    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.len() >= MIN_PHONE_DIGITS && long.ends_with(short)
}

fn same_name(a: &Keys, b: &Keys) -> bool {
    !a.names.is_disjoint(&b.names)
}

fn same_reach(a: &Keys, b: &Keys) -> bool {
    !a.emails.is_disjoint(&b.emails)
        || a.phones
            .iter()
            .any(|phone| b.phones.iter().any(|other| phones_match(phone, other)))
}

/// The outcome of matching one contact of the phone against the address book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Match {
    /// Exactly one candidate, with the same name and a same number or address.
    Clear(String),
    /// The person decides, per candidate.
    Uncertain(Vec<String>),
    None,
}

/// Matches a contact of the phone against the contacts of the address book (id and keys).
pub fn find(phone: &Keys, book: &[(String, Keys)]) -> Match {
    let mut clear = None;
    let mut candidates = Vec::new();
    for (id, keys) in book {
        let name = same_name(phone, keys);
        let reach = same_reach(phone, keys);
        if name && reach {
            clear = Some(id.clone());
        }
        if name || reach {
            candidates.push(id.clone());
        }
    }
    match (clear, candidates.len()) {
        (Some(id), 1) => Match::Clear(id),
        (_, 0) => Match::None,
        _ => Match::Uncertain(candidates),
    }
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;

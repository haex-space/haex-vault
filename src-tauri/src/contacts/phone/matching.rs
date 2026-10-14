//! Which new contacts of the phone merge into a contact of the address book (spec 051 FR-019,
//! research R8). The first run matches against every contact without a raw contact on this phone
//! and asks about uncertain pairs; a later run only links a new contact of the phone to a contact
//! that has this phone as a target and no raw contact on it yet, on a clear match (an account that
//! came back, say), and takes every other one over as a new contact.

use std::collections::{BTreeMap, BTreeSet};

use tauri_plugin_holzi_android::contacts::RawData;

use super::gather::NewGroup;
use super::mapping::{map_raw, PhoneSide};
use super::merge::{find, keys, Keys, Match};
use super::{Pair, PairDecision, PairSide};
use crate::contacts::ids::pair_id;
use crate::contacts::kinds::{EMAIL, PHONE};
use crate::contacts::model::ContactHeader;

/// What happens to each new group.
#[derive(Debug, Default)]
pub struct Matched {
    /// Group key → contact it merges into.
    pub merges: BTreeMap<String, String>,
    /// Uncertain pairs, for the overview.
    pub pairs: Vec<Pair>,
}

fn side_keys(sides: &[&PhoneSide]) -> (Keys, PairSide) {
    let mut out = Keys::default();
    let mut shown = PairSide::default();
    for side in sides {
        let head = |column: &str| side.head.get(column).map(String::as_str);
        let phones: Vec<&str> = side
            .values
            .iter()
            .filter(|value| value.kind == PHONE)
            .map(|value| value.value.as_str())
            .collect();
        let emails: Vec<&str> = side
            .values
            .iter()
            .filter(|value| value.kind == EMAIL)
            .map(|value| value.value.as_str())
            .collect();
        let found = keys(
            head("display_name"),
            head("given_name"),
            head("family_name"),
            &phones,
            &emails,
        );
        out.names.extend(found.names);
        out.phones.extend(found.phones);
        out.emails.extend(found.emails);
        if shown.name.is_empty() {
            shown.name = head("display_name")
                .map(str::to_owned)
                .or_else(|| {
                    let parts: Vec<&str> = [head("given_name"), head("family_name")]
                        .into_iter()
                        .flatten()
                        .collect();
                    (!parts.is_empty()).then(|| parts.join(" "))
                })
                .unwrap_or_default();
        }
        shown
            .phones
            .extend(phones.iter().map(|phone| (*phone).to_owned()));
        shown
            .emails
            .extend(emails.iter().map(|email| (*email).to_owned()));
    }
    (out, shown)
}

fn header_keys(header: &ContactHeader) -> Keys {
    let phones: Vec<&str> = header.phones.iter().map(String::as_str).collect();
    let emails: Vec<&str> = header.emails.iter().map(String::as_str).collect();
    keys(
        Some(header.name.as_str()),
        header.given_name.as_deref(),
        header.family_name.as_deref(),
        &phones,
        &emails,
    )
}

/// Matches the new groups against `book` (the contacts they may merge into). With `decisions`
/// (the first run) uncertain pairs merge where the person said so; without, only clear matches
/// merge and nothing is uncertain.
pub fn match_groups(
    groups: &[NewGroup],
    data: &BTreeMap<i64, RawData>,
    book: &[ContactHeader],
    decisions: Option<&[PairDecision]>,
) -> Matched {
    let book_keys: Vec<(String, Keys)> = book
        .iter()
        .map(|header| (header.id.clone(), header_keys(header)))
        .collect();
    let mut clear: BTreeMap<String, String> = BTreeMap::new();
    let mut uncertain: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut shown: BTreeMap<String, PairSide> = BTreeMap::new();
    for group in groups.iter().filter(|group| group.joins.is_none()) {
        let sides: Vec<PhoneSide> = group
            .raws
            .iter()
            .filter_map(|raw| data.get(&raw.id))
            .map(|raw| map_raw(&raw.data))
            .collect();
        let (keys, side) = side_keys(&sides.iter().collect::<Vec<_>>());
        shown.insert(group.key.clone(), side);
        match find(&keys, &book_keys) {
            Match::Clear(id) => {
                clear.insert(group.key.clone(), id);
            }
            Match::Uncertain(ids) => {
                uncertain.insert(group.key.clone(), ids);
            }
            Match::None => {}
        }
    }
    // A contact two groups claim clearly is uncertain for both.
    let mut claims: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (group, id) in &clear {
        claims.entry(id.clone()).or_default().push(group.clone());
    }
    for (id, groups) in claims.into_iter().filter(|(_, groups)| groups.len() > 1) {
        for group in groups {
            clear.remove(&group);
            uncertain.entry(group).or_default().push(id.clone());
        }
    }

    let mut out = Matched {
        merges: clear,
        pairs: Vec::new(),
    };
    let Some(decisions) = decisions else {
        return out;
    };
    let merge: BTreeSet<&str> = decisions
        .iter()
        .filter(|decision| decision.merge)
        .map(|decision| decision.pair_id.as_str())
        .collect();
    let headers: BTreeMap<&str, &ContactHeader> = book
        .iter()
        .map(|header| (header.id.as_str(), header))
        .collect();
    for (group, ids) in uncertain {
        for id in ids {
            let pair = pair_id(&group, &id);
            if merge.contains(pair.as_str()) && !out.merges.contains_key(&group) {
                out.merges.insert(group.clone(), id.clone());
            }
            let Some(header) = headers.get(id.as_str()) else {
                continue;
            };
            out.pairs.push(Pair {
                id: pair,
                phone: shown.get(&group).cloned().unwrap_or_default(),
                contact_id: id.clone(),
                contact: PairSide {
                    name: header.name.clone(),
                    phones: header.phones.clone(),
                    emails: header.emails.clone(),
                },
            });
        }
    }
    out
}

#[cfg(test)]
#[path = "matching_tests.rs"]
mod tests;

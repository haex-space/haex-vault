//! What changed on the phone since the last run (spec 051, research R6): the index of all raw
//! contacts against the links of this phone. A raw contact without a link is new, one with a
//! higher `VERSION` changed, one that is missing or marked deleted was deleted. The version after
//! holzi's own writes is stored with the link, so holzi's writes do not come back as changes.

use std::collections::BTreeMap;

use tauri_plugin_holzi_android::contacts::RawIndex;

use super::store::Link;

/// The raw contacts of the phone sorted by what happened to them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Classified {
    /// Raw contacts holzi has no link for.
    pub new: Vec<RawIndex>,
    /// Linked raw contacts whose version differs from the last run.
    pub changed: Vec<RawIndex>,
    /// Linked raw contacts with the version of the last run.
    pub unchanged: Vec<RawIndex>,
    /// Links whose raw contact is gone or marked deleted.
    pub gone: Vec<Link>,
}

/// Sorts the phone's index against the links of this phone.
pub fn classify(links: &[Link], index: &[RawIndex]) -> Classified {
    let mut by_raw: BTreeMap<i64, &Link> = links
        .iter()
        .map(|link| (link.raw_contact_id, link))
        .collect();
    let mut out = Classified::default();
    for raw in index {
        let link = by_raw.remove(&raw.id);
        match link {
            Some(link) if raw.deleted => out.gone.push(link.clone()),
            Some(link) if raw.version != link.version => out.changed.push(raw.clone()),
            Some(_) => out.unchanged.push(raw.clone()),
            None if raw.deleted => {}
            None => out.new.push(raw.clone()),
        }
    }
    out.gone.extend(by_raw.into_values().cloned());
    out
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;

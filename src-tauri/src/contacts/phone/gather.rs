//! The first half of a run (spec 051, research R6 to R9): what holzi and the phone hold now, and
//! what changed. Reads only; nothing here writes either side.

use std::collections::{BTreeMap, BTreeSet};

use tauri_plugin_holzi_android::contacts::{PhoneAccount, RawData, RawIndex};

use super::guard::vanished_accounts;
use super::index::{classify, Classified};
use super::store::{self, Link, Target};
use super::{PhoneBook, MAX_READ};
use crate::error::Result;
use crate::vault_gate::VaultDb;

/// Raw contacts per page of the index.
const INDEX_PAGE: u32 = 500;

/// Both sides at the start of a run.
pub struct Gathered {
    pub accounts: Vec<PhoneAccount>,
    pub index: BTreeMap<i64, RawIndex>,
    pub links: Vec<Link>,
    pub known: BTreeMap<i64, String>,
    pub targets: Vec<Target>,
    /// `updated_at` of every contact of holzi.
    pub updated_at: BTreeMap<String, String>,
    pub classified: Classified,
}

impl Gathered {
    /// Whether a raw contact may be written: not read-only, in a writable account.
    pub fn writable(&self, raw: &RawIndex) -> bool {
        !raw.read_only && self.account_writable(&raw.account_type, &raw.account_name)
    }

    pub fn account_writable(&self, account_type: &str, name: &str) -> bool {
        self.accounts.iter().any(|account| {
            account.account_type == account_type && account.name == name && account.writable
        })
    }

    /// The links of a contact.
    pub fn links_of<'a>(&'a self, contact_id: &'a str) -> impl Iterator<Item = &'a Link> + 'a {
        self.links
            .iter()
            .filter(move |link| link.contact_id == contact_id)
    }

    pub fn target_of(&self, contact_id: &str) -> Option<&Target> {
        self.targets
            .iter()
            .find(|target| target.contact_id == contact_id)
    }
}

/// Reads both sides.
pub fn gather(db: &VaultDb, book: &dyn PhoneBook, device: &str) -> Result<Gathered> {
    let accounts = book.accounts()?;
    let mut index = Vec::new();
    let mut after = 0;
    loop {
        let page = book.index(after, INDEX_PAGE)?;
        let Some(last) = page.last() else { break };
        after = last.id;
        let full = page.len() == INDEX_PAGE as usize;
        index.extend(page);
        if !full {
            break;
        }
    }
    let device = device.to_owned();
    let (links, known, targets, updated_at) = db.read_blocking(move |q| {
        Ok((
            store::links(q, &device)?,
            store::known(q)?,
            store::targets(q, &device)?,
            store::updated_at(q)?,
        ))
    })?;
    let classified = classify(&links, &index);
    Ok(Gathered {
        accounts,
        index: index.into_iter().map(|raw| (raw.id, raw)).collect(),
        links,
        known,
        targets,
        updated_at,
        classified,
    })
}

/// The deletions a run found, before the guard.
#[derive(Debug, Default)]
pub struct Deletions {
    /// Contacts whose raw contacts on this phone are all gone: deleted on the phone.
    pub from_phone: BTreeMap<String, Vec<i64>>,
    /// Raw contacts whose contact holzi deleted, by that contact.
    pub from_holzi: BTreeMap<String, Vec<i64>>,
    /// Links to drop without deleting anything: one raw contact of several gone, or its account
    /// left the phone (FR-026), or a raw contact gone whose contact no longer has this phone as a
    /// target.
    pub unlink: Vec<i64>,
    /// Raw contacts of contacts that no longer have this phone as a target, while their links
    /// stay: the user chose to delete them on the phone (FR-018d). Not held back by the guard,
    /// the user decided on exactly these.
    pub untargeted: BTreeMap<String, Vec<i64>>,
}

impl Deletions {
    pub fn count(&self) -> usize {
        self.from_phone.len() + self.from_holzi.len()
    }
}

/// Sorts what is gone on either side.
pub fn deletions(gathered: &Gathered) -> Deletions {
    let mut out = Deletions::default();
    let gone = &gathered.classified.gone;
    let vanished = vanished_accounts(
        gone,
        &gathered.index.values().cloned().collect::<Vec<_>>(),
        &gathered.accounts,
    );
    let gone_ids: BTreeSet<i64> = gone.iter().map(|link| link.raw_contact_id).collect();
    let targeted: BTreeSet<&str> = gathered
        .targets
        .iter()
        .map(|target| target.contact_id.as_str())
        .collect();
    for link in gone {
        let account = (link.account_type.clone(), link.account_name.clone());
        if vanished.contains(&account)
            || !gathered.updated_at.contains_key(&link.contact_id)
            || !targeted.contains(link.contact_id.as_str())
        {
            out.unlink.push(link.raw_contact_id);
            continue;
        }
        let all_gone = gathered
            .links_of(&link.contact_id)
            .all(|other| gone_ids.contains(&other.raw_contact_id));
        if all_gone {
            out.from_phone
                .entry(link.contact_id.clone())
                .or_default()
                .push(link.raw_contact_id);
        } else {
            out.unlink.push(link.raw_contact_id);
        }
    }
    // Links whose contact holzi deleted (when the delete did not take the link with it).
    for link in &gathered.links {
        let present = gathered
            .index
            .get(&link.raw_contact_id)
            .is_some_and(|raw| !raw.deleted);
        if !present {
            continue;
        }
        if !gathered.updated_at.contains_key(&link.contact_id) {
            out.from_holzi
                .entry(link.contact_id.clone())
                .or_default()
                .push(link.raw_contact_id);
        } else if !targeted.contains(link.contact_id.as_str()) {
            out.untargeted
                .entry(link.contact_id.clone())
                .or_default()
                .push(link.raw_contact_id);
        }
    }
    // Raw contacts this phone linked whose link went with a contact holzi deleted.
    for raw in &gathered.classified.new {
        if let Some(contact) = gathered.known.get(&raw.id) {
            if !gathered.updated_at.contains_key(contact) {
                out.from_holzi
                    .entry(contact.clone())
                    .or_default()
                    .push(raw.id);
            }
        }
    }
    out
}

/// The contacts linked on this phone that still exist.
pub fn linked_contacts(gathered: &Gathered) -> BTreeSet<String> {
    gathered
        .links
        .iter()
        .filter(|link| gathered.updated_at.contains_key(&link.contact_id))
        .map(|link| link.contact_id.clone())
        .collect()
}

/// The new raw contacts of the phone (not known to this phone), grouped by their aggregated
/// contact (research R7). A group whose aggregated contact already has a linked raw contact joins
/// that contact.
pub struct NewGroup {
    pub key: String,
    pub raws: Vec<RawIndex>,
    /// The contact a linked raw contact of the same aggregated contact belongs to.
    pub joins: Option<String>,
}

pub fn new_groups(gathered: &Gathered) -> Vec<NewGroup> {
    let mut joined: BTreeMap<&str, &str> = BTreeMap::new();
    for link in &gathered.links {
        if let Some(raw) = gathered.index.get(&link.raw_contact_id) {
            if !raw.deleted && !raw.lookup_key.is_empty() {
                joined.insert(&raw.lookup_key, &link.contact_id);
            }
        }
    }
    let mut groups: BTreeMap<String, Vec<RawIndex>> = BTreeMap::new();
    for raw in &gathered.classified.new {
        if gathered.known.contains_key(&raw.id) {
            continue;
        }
        let key = if raw.lookup_key.is_empty() {
            format!("raw:{}", raw.id)
        } else {
            raw.lookup_key.clone()
        };
        groups.entry(key).or_default().push(raw.clone());
    }
    groups
        .into_iter()
        .map(|(key, raws)| NewGroup {
            joins: joined.get(key.as_str()).map(|id| (*id).to_owned()),
            key,
            raws,
        })
        .collect()
}

/// Reads the data rows of raw contacts, in pages.
pub fn read_all(book: &dyn PhoneBook, ids: &[i64]) -> Result<BTreeMap<i64, RawData>> {
    let mut out = BTreeMap::new();
    for chunk in ids.chunks(MAX_READ) {
        for raw in book.read(chunk)? {
            out.insert(raw.id, raw);
        }
    }
    Ok(out)
}

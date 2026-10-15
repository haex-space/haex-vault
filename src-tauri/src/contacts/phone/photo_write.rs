//! Photos from holzi to the phone (spec 051 T054, research R10): a contact's photo is written to
//! the phone when it differs from the photo of the last run (`photo_base`), compared by hash, not
//! by the phone's picture, which the provider scales on writing. A contact taken over or merged
//! agrees with the phone as it is; a photo removed in holzi stays on the phone.

use std::collections::BTreeMap;

use super::apply::Planned;
use super::gather::Gathered;
use super::jobs::primary;
use super::sides::Loaded;
use super::store::Target;
use super::PhoneBook;
use crate::contacts::photos;
use crate::error::Result;
use crate::vault_gate::VaultDb;

/// Where a photo goes: the contact, its raw contact on the phone and the photo's hash.
struct Pending {
    contact_id: String,
    raw: i64,
    hash: String,
    created: bool,
}

/// What the photo writes changed: the hash written per contact, and the versions afterwards.
#[derive(Default)]
pub struct Written {
    pub hashes: BTreeMap<String, String>,
    pub versions: BTreeMap<i64, i64>,
}

fn pending(
    gathered: &Gathered,
    planned: &[Planned],
    creates: &[&Target],
    loaded: &BTreeMap<String, Loaded>,
    created: &BTreeMap<String, (i64, i64)>,
) -> Vec<Pending> {
    let mut out = Vec::new();
    for item in planned.iter().filter(|item| !(item.import || item.merge)) {
        let Some(hash) = item.loaded.as_ref().and_then(|loaded| loaded.photo.clone()) else {
            continue;
        };
        let base = gathered
            .links_of(&item.contact_id)
            .find_map(|link| link.photo_base.clone());
        if base.as_deref() == Some(hash.as_str()) {
            continue;
        }
        let target = gathered.target_of(&item.contact_id);
        if let Some(raw) = primary(&item.raws, &item.index, target) {
            out.push(Pending {
                contact_id: item.contact_id.clone(),
                raw,
                hash,
                created: false,
            });
        }
    }
    for target in creates {
        let photo = loaded
            .get(&target.contact_id)
            .and_then(|contact| contact.photo.clone());
        if let (Some(hash), Some((raw, _))) = (photo, created.get(&target.contact_id)) {
            out.push(Pending {
                contact_id: target.contact_id.clone(),
                raw: *raw,
                hash,
                created: true,
            });
        }
    }
    out
}

/// Writes the photos that changed in holzi. A photo the phone refuses is logged and tried again
/// on the next run (its `photo_base` stays).
pub fn write(
    db: &VaultDb,
    book: &dyn PhoneBook,
    gathered: &Gathered,
    planned: &[Planned],
    creates: &[&Target],
    loaded: &BTreeMap<String, Loaded>,
    created: &mut BTreeMap<String, (i64, i64)>,
) -> Result<Written> {
    let pending = pending(gathered, planned, creates, loaded, created);
    let mut written = Written::default();
    if pending.is_empty() {
        return Ok(written);
    }
    let hashes: Vec<String> = pending.iter().map(|photo| photo.hash.clone()).collect();
    let bytes: BTreeMap<String, Vec<u8>> = db.read_blocking(move |q| {
        let mut out = BTreeMap::new();
        for hash in hashes {
            if let Ok(jpeg) = photos::data(q, &hash) {
                out.insert(hash, jpeg);
            }
        }
        Ok(out)
    })?;
    for photo in pending {
        let Some(jpeg) = bytes.get(&photo.hash) else {
            continue;
        };
        match book.write_photo(photo.raw, jpeg) {
            Ok(version) => {
                if let Some(version) = version {
                    if photo.created {
                        created.insert(photo.contact_id.clone(), (photo.raw, version));
                    } else {
                        written.versions.insert(photo.raw, version);
                    }
                }
                written.hashes.insert(photo.contact_id, photo.hash);
            }
            Err(error) => log::warn!("contacts: the phone refused a photo: {error}"),
        }
    }
    Ok(written)
}

#[cfg(test)]
#[path = "photo_write_tests.rs"]
mod tests;

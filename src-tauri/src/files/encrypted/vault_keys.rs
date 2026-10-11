//! The vault content keys behind encrypted folders (spec 048, research R1): `KEK_g` comes from the
//! content key `VCK_g` of generation `g` (`sync::content_keys`). A new folder takes the current
//! generation; an existing one opens with whichever generation its head names, so every generation
//! this device holds counts, not only the few `listening_keys` returns.

use zeroize::Zeroizing;

use crate::files::encrypted::format::Id;
use crate::files::{FilesError, FilesErrorCode};
use crate::storage::query::Query;
use crate::sync::content_keys::{current_key, current_key_for_list, ContentKey};
use crate::sync::{device_list, envelopes, keys};

/// The error of a device without a content key (before the first data sync).
pub fn no_key() -> FilesError {
    FilesError::new(
        FilesErrorCode::NoKey,
        "this device holds no content key of the vault yet",
    )
}

/// The generation a new folder is wrapped for: the highest held one that names no device the
/// effective device list removed (as presence picks it), else the highest held one.
pub fn current(q: &mut impl Query) -> haex_crdt::Result<Option<ContentKey>> {
    let Some(vault) = keys::vault_pubkey(q)? else {
        return current_key(q, &[]);
    };
    let valid = device_list::valid_lists(&device_list::load_all(q)?, &vault);
    let Some(effective) = device_list::effective(&valid) else {
        return current_key(q, &[]);
    };
    let removed: Vec<[u8; 32]> = effective
        .list
        .removed
        .iter()
        .map(|removed| removed.device_pubkey)
        .collect();
    current_key_for_list(q, &removed, Some(&effective.hash))
}

/// Every generation this device holds, by key id: what opens heads.
pub struct HeldKeys(Vec<ContentKey>);

impl HeldKeys {
    pub fn load(q: &mut impl Query) -> haex_crdt::Result<Self> {
        Ok(Self(envelopes::held_keys(q)?))
    }

    /// The content key of generation `key_id`, if this device holds it.
    pub fn by_key_id(&self, key_id: &Id) -> Option<Zeroizing<[u8; 32]>> {
        self.0
            .iter()
            .find(|key| key.key_id == *key_id)
            .map(|key| Zeroizing::new(*key.key))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
#[path = "vault_keys_tests.rs"]
mod tests;

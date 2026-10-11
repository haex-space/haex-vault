//! The keys of an encrypted folder (format "Schlüssel"). `kdf(ikm, info)` is always HKDF-SHA256
//! with the 5-byte salt `holzi` and 32 bytes of output, the same construction as the vault identity
//! (`sync::keys::derive_vault_identity`). New folder keys, content keys and ids come from
//! [`crate::sync::keys::random_bytes`].

use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

const SALT: &[u8] = b"holzi";
const KEK_INFO: &[u8] = b"holzi/encrypted-folder/kek/v1";
const META_INFO: &[u8] = b"holzi/encrypted-folder/meta/v1";
const WRAP_INFO: &[u8] = b"holzi/encrypted-folder/wrap/v1";

/// `KEK_g` from the vault content key `VCK_g` of one generation: wraps folder keys in heads.
pub fn kek(vck: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    kdf(vck, KEK_INFO)
}

/// The two subkeys of a folder key `FK`.
pub struct FolderKeys {
    /// `K_meta`: the name in the head and the sidecars.
    pub meta: Zeroizing<[u8; 32]>,
    /// `K_wrap`: wraps each content object's `DEK` in its sidecar.
    pub wrap: Zeroizing<[u8; 32]>,
}

impl FolderKeys {
    pub fn derive(fk: &[u8; 32]) -> Self {
        Self {
            meta: kdf(fk, META_INFO),
            wrap: kdf(fk, WRAP_INFO),
        }
    }
}

fn kdf(ikm: &[u8; 32], info: &[u8]) -> Zeroizing<[u8; 32]> {
    let mut okm = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(Some(SALT), ikm)
        .expand(info, okm.as_mut())
        .expect("32 bytes are a valid HKDF-SHA256 output length");
    okm
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;

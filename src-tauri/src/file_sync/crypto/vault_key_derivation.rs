//! HKDF derivation of the own-vault file-encryption key from the
//! default-identity Ed25519 seed.
//!
//! The vault's default identity (`haex_identities` row with `source='own'`
//! and a non-null `private_key`) is per-vault, not per-device, and is
//! synchronised across devices via CRDT sync. Deriving the file key from
//! it therefore gives every device that holds this vault the same
//! encryption key without any extra transport — a device that opens the
//! vault runs the same HKDF over the same seed and lands on the same
//! 32-byte output.
//!
//! The salt and info strings are versioned so a future rotation can bump
//! `v1` to `v2` without conflating the old and new key spaces. The
//! domain-separation suffix `haex-file-encryption-v1` matches the sibling
//! separator used by [`super::key_resolver::derive_file_key`] for the
//! shared-space path (the two derivations still land in different key
//! spaces because their IKMs are disjoint).
//!
//! HKDF is Extract-then-Expand (RFC 5869): the extract step whitens the
//! Ed25519 seed with the salt (guarding against biased IKM), and the
//! expand step stretches the pseudorandom key to 32 bytes labelled by the
//! info string. Expand-only would skip the extract step and rely on the
//! seed already being pseudorandom — true here in practice but brittle
//! against future callers that reuse the module with different IKMs.

use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

/// Fixed salt for the HKDF extract step. Versioned so a future rotation
/// can bump to `-v2` without conflating key spaces.
const HKDF_SALT: &[u8] = b"haex-vault-file-key-v1-salt";

/// Info label for the HKDF expand step. Shares the `haex-file-encryption-v1`
/// domain with the shared-space key derivation — the two paths land on
/// disjoint key spaces because their IKMs (Ed25519 seed vs. MLS epoch
/// key) never overlap.
const HKDF_INFO: &[u8] = b"haex-file-encryption-v1";

/// Length in bytes of the derived file-encryption key.
pub const VAULT_FILE_KEY_LEN: usize = 32;

/// Derive the 32-byte own-vault file-encryption key from a 32-byte
/// Ed25519 seed. Deterministic — running the same seed through this
/// function on any device yields the same key, which is exactly what
/// makes multi-device same-vault cloud sync work without extra key
/// transport.
///
/// The output is wrapped in [`Zeroizing`] so the caller cannot forget
/// to scrub the buffer — dropping the returned value zeroes it in
/// place.
pub fn derive_vault_file_key(did_seed: &[u8; 32]) -> Zeroizing<[u8; VAULT_FILE_KEY_LEN]> {
    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), did_seed);
    let mut out = Zeroizing::new([0u8; VAULT_FILE_KEY_LEN]);
    hk.expand(HKDF_INFO, out.as_mut())
        .expect("HKDF-SHA256 expand for 32 bytes cannot fail (well below 255 * HashLen)");
    out
}

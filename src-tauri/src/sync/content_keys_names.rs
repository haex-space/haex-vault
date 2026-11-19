//! Sealed device names in the device list (spec 024, research R9): a name is
//! encrypted under a key derived from a content key generation, so only the
//! devices of the vault read it. Part of [`super`].

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use super::{ContentKey, KeyError, NONCE_LEN};
use crate::sync::keys::random_bytes;

/// Seals a device name for the device list: XChaCha20-Poly1305 under
/// `HKDF(key, "haex/device-name/v1")`, additional data `generation ‖
/// device_pubkey`. Output: `key_id ‖ nonce ‖ ciphertext`, so a reader knows
/// which generation opens it.
pub fn seal_name(key: &ContentKey, device_pubkey: &[u8; 32], name: &str) -> Vec<u8> {
    let nonce = random_bytes::<NONCE_LEN>();
    let ciphertext = name_cipher(key)
        .encrypt(
            XNonce::from_slice(nonce.as_slice()),
            Payload {
                msg: name.as_bytes(),
                aad: &name_aad(key.generation, device_pubkey),
            },
        )
        .expect("XChaCha20-Poly1305 encrypts any name");
    let mut sealed = Vec::with_capacity(16 + NONCE_LEN + ciphertext.len());
    sealed.extend_from_slice(&key.key_id);
    sealed.extend_from_slice(nonce.as_slice());
    sealed.extend_from_slice(&ciphertext);
    sealed
}

/// Opens a name sealed by [`seal_name`] with the key of its generation.
pub fn open_name(
    key: &ContentKey,
    device_pubkey: &[u8; 32],
    sealed: &[u8],
) -> Result<String, KeyError> {
    if sealed.len() < 16 + NONCE_LEN || sealed[..16] != key.key_id {
        return Err(KeyError::SealedName);
    }
    let nonce = &sealed[16..16 + NONCE_LEN];
    let plaintext = name_cipher(key)
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: &sealed[16 + NONCE_LEN..],
                aad: &name_aad(key.generation, device_pubkey),
            },
        )
        .map_err(|_| KeyError::SealedName)?;
    String::from_utf8(plaintext).map_err(|_| KeyError::SealedName)
}

/// The key id a sealed name was sealed with.
pub fn sealed_name_key_id(sealed: &[u8]) -> Option<[u8; 16]> {
    sealed.get(..16)?.try_into().ok()
}

/// Derives the device-name encryption key with HKDF-SHA256 and initializes
/// an XChaCha20-Poly1305 cipher.
fn name_cipher(key: &ContentKey) -> XChaCha20Poly1305 {
    let hkdf = Hkdf::<Sha256>::new(None, key.key.as_slice());
    let mut derived = Zeroizing::new([0u8; 32]);
    hkdf.expand(b"haex/device-name/v1", derived.as_mut())
        .expect("32 bytes are a valid HKDF-SHA256 output length");
    XChaCha20Poly1305::new_from_slice(derived.as_slice()).expect("a 32-byte key")
}

/// Encodes the name's authenticated data as the big-endian key generation
/// followed by the device public key.
fn name_aad(generation: u64, device_pubkey: &[u8; 32]) -> Vec<u8> {
    let mut aad = generation.to_be_bytes().to_vec();
    aad.extend_from_slice(device_pubkey);
    aad
}

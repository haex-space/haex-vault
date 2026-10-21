//! The envelopes of the bridge (spec 049, research R3, `contracts/wire-protocol.md`): X25519 with a
//! fresh key per message and AES-256-GCM, byte for byte as haex-pass-browser's
//! `background/crypto.ts` and haex-vault 2.x (Legacy) speak them. Keys and ciphertexts travel as
//! standard Base64 with padding.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use curve25519_dalek::montgomery::MontgomeryPoint;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

/// Length of an X25519 key.
pub const KEY_LEN: usize = 32;
/// Length of the AES-GCM nonce the client sends.
pub const IV_LEN: usize = 12;

/// Why an envelope could not be opened or a key not be read. Carries no content (FR-039).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EnvelopeError {
    #[error("not valid Base64")]
    Base64,
    #[error("a key is not 32 bytes")]
    KeyLength,
    #[error("the IV is not 12 bytes")]
    IvLength,
    #[error("the key agreement gave a low-order point")]
    LowOrder,
    #[error("the ciphertext does not authenticate")]
    Decrypt,
}

/// The key pair of a running bridge: random at every start, only in memory, erased on drop.
pub struct ServerKeys {
    secret: Zeroizing<[u8; KEY_LEN]>,
    public: [u8; KEY_LEN],
}

impl ServerKeys {
    pub fn generate() -> Self {
        Self::from_secret(random_bytes())
    }

    /// A fixed key pair, for tests and fixtures.
    pub fn from_secret(secret: [u8; KEY_LEN]) -> Self {
        let public = MontgomeryPoint::mul_base_clamped(secret).to_bytes();
        Self {
            secret: Zeroizing::new(secret),
            public,
        }
    }

    /// The public key as the handshake answer carries it (`serverPublicKey`).
    pub fn public_b64(&self) -> String {
        STANDARD.encode(self.public)
    }

    /// Opens a request: the client sealed it to this key with its own fresh key `ephemeral_b64`.
    pub fn open_request(
        &self,
        ephemeral_b64: &str,
        iv_b64: &str,
        message_b64: &str,
    ) -> Result<Vec<u8>, EnvelopeError> {
        let ephemeral = decode_key(ephemeral_b64)?;
        let key = agree(&self.secret, &ephemeral)?;
        let iv = STANDARD.decode(iv_b64).map_err(|_| EnvelopeError::Base64)?;
        let message = STANDARD
            .decode(message_b64)
            .map_err(|_| EnvelopeError::Base64)?;
        open(&key, &iv, &message)
    }
}

/// A sealed answer: the fresh public key, the IV and the ciphertext, each in standard Base64.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    pub public_key: String,
    pub iv: String,
    pub message: String,
}

/// Seals an answer to the client's handshake key with a fresh key of ours.
pub fn seal_response(
    client_public: &[u8; KEY_LEN],
    plaintext: &[u8],
) -> Result<Sealed, EnvelopeError> {
    seal_with(random_bytes(), random_bytes(), client_public, plaintext)
}

/// [`seal_response`] with a fixed fresh key and IV, for the WebCrypto fixture (research R18).
pub fn seal_with(
    ephemeral_secret: [u8; KEY_LEN],
    iv: [u8; IV_LEN],
    client_public: &[u8; KEY_LEN],
    plaintext: &[u8],
) -> Result<Sealed, EnvelopeError> {
    let ephemeral_secret = Zeroizing::new(ephemeral_secret);
    let ephemeral_public = MontgomeryPoint::mul_base_clamped(*ephemeral_secret).to_bytes();
    let key = agree(&ephemeral_secret, client_public)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| EnvelopeError::KeyLength)?;
    let message = cipher
        .encrypt(&Nonce::from(iv), plaintext)
        .map_err(|_| EnvelopeError::Decrypt)?;
    Ok(Sealed {
        public_key: STANDARD.encode(ephemeral_public),
        iv: STANDARD.encode(iv),
        message: STANDARD.encode(message),
    })
}

/// The fingerprint of a client: the first 16 bytes of SHA-256 over its raw public key, as 32
/// lowercase hex digits (`background/crypto.ts`). holzi checks it against the claimed id (FR-006).
pub fn client_id_of(public_key: &[u8]) -> Result<String, EnvelopeError> {
    if public_key.len() != KEY_LEN {
        return Err(EnvelopeError::KeyLength);
    }
    let digest = Sha256::digest(public_key);
    Ok(digest[..16].iter().map(|b| format!("{b:02x}")).collect())
}

/// Reads a public key from standard Base64.
pub fn decode_key(b64: &str) -> Result<[u8; KEY_LEN], EnvelopeError> {
    let bytes = STANDARD.decode(b64).map_err(|_| EnvelopeError::Base64)?;
    bytes.try_into().map_err(|_| EnvelopeError::KeyLength)
}

/// X25519 per RFC 7748. A shared secret of only zero bytes means the peer sent a point of low
/// order; it is refused, so no message is ever sealed under a key an attacker can predict.
// ponytail: the raw shared secret is the AES key, with no KDF and no associated data, because the
// unchanged client does it so (research R3). Ceiling: departs from RFC 7748 §6.1's advice; every
// message has its own key and a random IV, so no nonce is ever reused. Upgrade path: a protocol v3
// with HKDF and the action as associated data, agreed with the extension.
pub fn agree(
    secret: &[u8; KEY_LEN],
    public: &[u8; KEY_LEN],
) -> Result<Zeroizing<[u8; KEY_LEN]>, EnvelopeError> {
    let mut shared = MontgomeryPoint(*public).mul_clamped(*secret).to_bytes();
    if shared.iter().all(|b| *b == 0) {
        shared.zeroize();
        return Err(EnvelopeError::LowOrder);
    }
    Ok(Zeroizing::new(shared))
}

fn open(key: &[u8; KEY_LEN], iv: &[u8], message: &[u8]) -> Result<Vec<u8>, EnvelopeError> {
    let iv: [u8; IV_LEN] = iv.try_into().map_err(|_| EnvelopeError::IvLength)?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EnvelopeError::KeyLength)?;
    cipher
        .decrypt(&Nonce::from(iv), message)
        .map_err(|_| EnvelopeError::Decrypt)
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("the operating system's random source is available");
    bytes
}

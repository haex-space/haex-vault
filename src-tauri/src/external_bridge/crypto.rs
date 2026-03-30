//! Cryptographic operations for browser bridge communication
//!
//! Uses X25519 for key exchange and AES-256-GCM for encryption.
//! Compatible with WebCrypto API in browsers.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use x25519_dalek::{PublicKey, StaticSecret};

use super::error::BridgeError;

const IV_LENGTH: usize = 12;
const X25519_PUBLIC_KEY_LENGTH: usize = 32;

/// Server keypair for X25519 key exchange
pub struct ServerKeyPair {
    secret: StaticSecret,
    public_key: PublicKey,
}

impl ServerKeyPair {
    /// Generate a new X25519 keypair
    pub fn generate() -> Self {
        let mut secret_bytes = [0u8; 32];
        rand::fill(&mut secret_bytes);
        let secret = StaticSecret::from(secret_bytes);
        let public_key = PublicKey::from(&secret);
        Self { secret, public_key }
    }

    /// Export public key as Base64 raw format (32 bytes)
    pub fn public_key_base64(&self) -> String {
        BASE64.encode(self.public_key.as_bytes())
    }

    /// Derive shared secret with a client's public key
    pub fn derive_shared_secret(&self, client_public_key: &PublicKey) -> [u8; 32] {
        let shared_secret = self.secret.diffie_hellman(client_public_key);
        shared_secret.to_bytes()
    }
}

/// Import a public key from Base64 raw format (32 bytes)
pub fn import_public_key(base64_key: &str) -> Result<PublicKey, BridgeError> {
    let key_bytes = BASE64
        .decode(base64_key)
        .map_err(|e| BridgeError::Crypto(format!("Invalid base64: {}", e)))?;

    if key_bytes.len() != X25519_PUBLIC_KEY_LENGTH {
        return Err(BridgeError::Crypto(format!(
            "Invalid X25519 public key length: expected {}, got {}",
            X25519_PUBLIC_KEY_LENGTH,
            key_bytes.len()
        )));
    }

    let mut key_array = [0u8; X25519_PUBLIC_KEY_LENGTH];
    key_array.copy_from_slice(&key_bytes);
    Ok(PublicKey::from(key_array))
}

/// Encrypt a message using AES-256-GCM
pub fn encrypt_message(plaintext: &[u8], shared_key: &[u8; 32]) -> Result<(Vec<u8>, [u8; IV_LENGTH]), BridgeError> {
    let cipher = Aes256Gcm::new_from_slice(shared_key)
        .map_err(|e| BridgeError::Crypto(format!("Invalid key: {}", e)))?;

    let mut iv = [0u8; IV_LENGTH];
    rand::fill(&mut iv);
    let nonce = Nonce::from(iv);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| BridgeError::Crypto(format!("Encryption failed: {}", e)))?;

    Ok((ciphertext, iv))
}

/// Decrypt a message using AES-256-GCM
pub fn decrypt_message(
    ciphertext: &[u8],
    iv: &[u8; IV_LENGTH],
    shared_key: &[u8; 32],
) -> Result<Vec<u8>, BridgeError> {
    let cipher = Aes256Gcm::new_from_slice(shared_key)
        .map_err(|e| BridgeError::Crypto(format!("Invalid key: {}", e)))?;

    let nonce = Nonce::from(*iv);

    cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|e| BridgeError::Crypto(format!("Decryption failed: {}", e)))
}

/// Encrypted message envelope (matches browser extension format)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedEnvelope {
    pub action: String,
    pub message: String,      // Base64 encrypted payload
    pub iv: String,           // Base64 12-byte IV
    pub client_id: String,
    pub public_key: String,   // Ephemeral public key (Base64 raw 32 bytes)
    /// Target extension's public key (from manifest) - identifies the developer
    #[serde(default)]
    pub extension_public_key: Option<String>,
    /// Target extension's name (from manifest) - together with public_key uniquely identifies the extension
    #[serde(default)]
    pub extension_name: Option<String>,
}

impl EncryptedEnvelope {
    /// Decrypt this envelope using the server's private key
    pub fn decrypt(&self, server_keypair: &ServerKeyPair) -> Result<serde_json::Value, BridgeError> {
        // Import client's ephemeral public key
        let client_public_key = import_public_key(&self.public_key)?;

        // Derive shared secret
        let shared_secret = server_keypair.derive_shared_secret(&client_public_key);

        // Decode base64 fields
        let ciphertext = BASE64
            .decode(&self.message)
            .map_err(|e| BridgeError::Crypto(format!("Invalid ciphertext base64: {}", e)))?;

        let iv_bytes = BASE64
            .decode(&self.iv)
            .map_err(|e| BridgeError::Crypto(format!("Invalid IV base64: {}", e)))?;

        if iv_bytes.len() != IV_LENGTH {
            return Err(BridgeError::Crypto(format!(
                "Invalid IV length: expected {}, got {}",
                IV_LENGTH,
                iv_bytes.len()
            )));
        }

        let mut iv = [0u8; IV_LENGTH];
        iv.copy_from_slice(&iv_bytes);

        // Decrypt
        let plaintext = decrypt_message(&ciphertext, &iv, &shared_secret)?;

        // Parse JSON
        serde_json::from_slice(&plaintext)
            .map_err(|e| BridgeError::Crypto(format!("Invalid JSON in decrypted message: {}", e)))
    }
}

/// Create an encrypted response envelope
pub fn create_encrypted_response(
    action: &str,
    payload: &serde_json::Value,
    client_public_key_base64: &str,
) -> Result<EncryptedEnvelope, BridgeError> {
    // Generate ephemeral keypair for forward secrecy
    let ephemeral = ServerKeyPair::generate();

    // Import client's public key
    let client_public_key = import_public_key(client_public_key_base64)?;

    // Derive shared secret
    let shared_secret = ephemeral.derive_shared_secret(&client_public_key);

    // Serialize payload
    let plaintext = serde_json::to_vec(payload)
        .map_err(|e| BridgeError::Crypto(format!("Failed to serialize payload: {}", e)))?;

    // Encrypt
    let (ciphertext, iv) = encrypt_message(&plaintext, &shared_secret)?;

    Ok(EncryptedEnvelope {
        action: action.to_string(),
        message: BASE64.encode(&ciphertext),
        iv: BASE64.encode(&iv),
        client_id: String::new(),
        public_key: ephemeral.public_key_base64(),
        extension_public_key: None,
        extension_name: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let keypair = ServerKeyPair::generate();
        let pub_base64 = keypair.public_key_base64();
        assert!(!pub_base64.is_empty());
        let decoded = BASE64.decode(&pub_base64).unwrap();
        assert_eq!(decoded.len(), X25519_PUBLIC_KEY_LENGTH);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0u8; 32];
        let plaintext = b"Hello, World!";

        let (ciphertext, iv) = encrypt_message(plaintext, &key).unwrap();
        let decrypted = decrypt_message(&ciphertext, &iv, &key).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_key_import_export() {
        let keypair = ServerKeyPair::generate();
        let pub_base64 = keypair.public_key_base64();
        let imported = import_public_key(&pub_base64).unwrap();

        assert_eq!(keypair.public_key.as_bytes(), imported.as_bytes());
    }

    #[test]
    fn test_shared_secret_agreement() {
        let alice = ServerKeyPair::generate();
        let bob = ServerKeyPair::generate();

        let shared_a = alice.derive_shared_secret(&bob.public_key);
        let shared_b = bob.derive_shared_secret(&alice.public_key);

        assert_eq!(shared_a, shared_b);
    }
}

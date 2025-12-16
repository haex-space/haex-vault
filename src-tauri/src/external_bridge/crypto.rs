//! Cryptographic operations for browser bridge communication
//!
//! Uses ECDH (P-256) for key exchange and AES-256-GCM for encryption.
//! Compatible with WebCrypto API in browsers.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use p256::{
    ecdh::EphemeralSecret,
    elliptic_curve::sec1::ToEncodedPoint,
    pkcs8::EncodePublicKey,
    PublicKey,
};
use rand::rngs::OsRng;

use super::error::BridgeError;

const IV_LENGTH: usize = 12;

/// Server keypair for ECDH key exchange
pub struct ServerKeyPair {
    secret: EphemeralSecret,
    public_key: PublicKey,
}

impl ServerKeyPair {
    /// Generate a new ECDH keypair
    pub fn generate() -> Self {
        let secret = EphemeralSecret::random(&mut OsRng);
        let public_key = secret.public_key();
        Self { secret, public_key }
    }

    /// Export public key as Base64 SPKI format (compatible with WebCrypto)
    pub fn public_key_spki_base64(&self) -> Result<String, BridgeError> {
        let spki_der = self
            .public_key
            .to_public_key_der()
            .map_err(|e| BridgeError::Crypto(format!("Failed to encode SPKI: {}", e)))?;
        Ok(BASE64.encode(spki_der.as_bytes()))
    }

    /// Derive shared secret with a client's public key
    pub fn derive_shared_secret(&self, client_public_key: &PublicKey) -> [u8; 32] {
        let shared_secret = self.secret.diffie_hellman(client_public_key);
        let bytes = shared_secret.raw_secret_bytes();
        let mut result = [0u8; 32];
        result.copy_from_slice(bytes.as_slice());
        result
    }
}

/// Import a public key from Base64 SPKI format
pub fn import_public_key_spki(base64_spki: &str) -> Result<PublicKey, BridgeError> {
    use p256::pkcs8::DecodePublicKey;

    let der_bytes = BASE64
        .decode(base64_spki)
        .map_err(|e| BridgeError::Crypto(format!("Invalid base64: {}", e)))?;

    PublicKey::from_public_key_der(&der_bytes)
        .map_err(|e| BridgeError::Crypto(format!("Invalid SPKI public key: {}", e)))
}

/// Encrypt a message using AES-256-GCM
pub fn encrypt_message(plaintext: &[u8], shared_key: &[u8; 32]) -> Result<(Vec<u8>, [u8; IV_LENGTH]), BridgeError> {
    let cipher = Aes256Gcm::new_from_slice(shared_key)
        .map_err(|e| BridgeError::Crypto(format!("Invalid key: {}", e)))?;

    let mut iv = [0u8; IV_LENGTH];
    rand::RngCore::fill_bytes(&mut OsRng, &mut iv);
    let nonce = Nonce::from_slice(&iv);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
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

    let nonce = Nonce::from_slice(iv);

    cipher
        .decrypt(nonce, ciphertext)
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
    pub public_key: String,   // Ephemeral public key (SPKI base64)
}

impl EncryptedEnvelope {
    /// Decrypt this envelope using the server's private key
    pub fn decrypt(&self, server_keypair: &ServerKeyPair) -> Result<serde_json::Value, BridgeError> {
        // Import client's ephemeral public key
        let client_public_key = import_public_key_spki(&self.public_key)?;

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
    client_public_key_spki: &str,
) -> Result<EncryptedEnvelope, BridgeError> {
    // Generate ephemeral keypair for forward secrecy
    let ephemeral = ServerKeyPair::generate();

    // Import client's public key
    let client_public_key = import_public_key_spki(client_public_key_spki)?;

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
        client_id: String::new(), // Server doesn't have a client_id
        public_key: ephemeral.public_key_spki_base64()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let keypair = ServerKeyPair::generate();
        let spki = keypair.public_key_spki_base64().unwrap();
        assert!(!spki.is_empty());
        // SPKI format should be decodable
        let decoded = BASE64.decode(&spki).unwrap();
        assert!(!decoded.is_empty());
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0u8; 32]; // Test key
        let plaintext = b"Hello, World!";

        let (ciphertext, iv) = encrypt_message(plaintext, &key).unwrap();
        let decrypted = decrypt_message(&ciphertext, &iv, &key).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_spki_import_export() {
        let keypair = ServerKeyPair::generate();
        let spki = keypair.public_key_spki_base64().unwrap();
        let imported = import_public_key_spki(&spki).unwrap();

        // Verify the imported key matches by comparing encoded points
        let original_point = keypair.public_key.to_encoded_point(false);
        let imported_point = imported.to_encoded_point(false);
        assert_eq!(original_point.as_bytes(), imported_point.as_bytes());
    }
}

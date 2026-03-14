//! Device key management
//!
//! Each device has a unique Ed25519 keypair stored in the app data directory.
//! The keypair is generated on first vault open and persists across restarts.
//! The public key (EndpointId) serves as a cryptographically strong device identifier.
//!
//! Storage: `<app_data>/devices/<vault-uuid>.key`
//! The key file is encrypted with a 32-byte secret stored in the vault database.

use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use iroh::SecretKey;

use crate::device::error::DeviceError;

const NONCE_SIZE: usize = 12;

/// Resolve the path to the device key file for a given vault UUID.
fn device_key_path(app_data_dir: &Path, vault_uuid: &str) -> PathBuf {
    app_data_dir.join("devices").join(format!("{vault_uuid}.key"))
}

/// Load or generate a device key for the given vault.
///
/// - If a key file exists, decrypt and return it.
/// - If no key file exists, generate a new keypair, encrypt and save it.
///
/// `encryption_key` is a 32-byte secret stored in the vault database.
pub fn load_or_generate(
    app_data_dir: &Path,
    vault_uuid: &str,
    encryption_key: &[u8; 32],
) -> Result<SecretKey, DeviceError> {
    let path = device_key_path(app_data_dir, vault_uuid);

    if path.exists() {
        load(&path, encryption_key)
    } else {
        let secret_key = generate_new();
        save(&path, &secret_key, encryption_key)?;
        Ok(secret_key)
    }
}

fn generate_new() -> SecretKey {
    // TODO: Replace with SecretKey::generate() once p256 upgrades to rand_core 0.9
    // Currently iroh uses rand_core 0.9 but p256 uses rand_core 0.6 (via rand 0.8).
    // Track: when p256 >= 0.14 ships with rand_core 0.9, upgrade rand to 0.9 project-wide.
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
    SecretKey::from_bytes(&bytes)
}

fn load(path: &Path, encryption_key: &[u8; 32]) -> Result<SecretKey, DeviceError> {
    let encrypted = fs::read(path)?;

    if encrypted.len() < NONCE_SIZE {
        return Err(DeviceError::KeyError {
            reason: "Device key file too short".to_string(),
        });
    }

    let (nonce_bytes, ciphertext) = encrypted.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(encryption_key).map_err(|e| {
        DeviceError::Encryption {
            reason: format!("Invalid encryption key: {e}"),
        }
    })?;

    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        DeviceError::Encryption {
            reason: "Failed to decrypt device key — encryption key may have changed".to_string(),
        }
    })?;

    if plaintext.len() != 32 {
        return Err(DeviceError::KeyError {
            reason: format!("Invalid device key length: expected 32, got {}", plaintext.len()),
        });
    }

    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&plaintext);
    Ok(SecretKey::from_bytes(&key_bytes))
}

fn save(
    path: &Path,
    secret_key: &SecretKey,
    encryption_key: &[u8; 32],
) -> Result<(), DeviceError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let cipher = Aes256Gcm::new_from_slice(encryption_key).map_err(|e| {
        DeviceError::Encryption {
            reason: format!("Invalid encryption key: {e}"),
        }
    })?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = secret_key.to_bytes();
    let ciphertext = cipher.encrypt(nonce, plaintext.as_slice()).map_err(|e| {
        DeviceError::Encryption {
            reason: format!("Failed to encrypt device key: {e}"),
        }
    })?;

    // File format: [12 bytes nonce][ciphertext]
    let mut data = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    data.extend_from_slice(&nonce_bytes);
    data.extend_from_slice(&ciphertext);

    fs::write(path, &data)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_and_load() {
        let dir = TempDir::new().unwrap();
        let encryption_key = [42u8; 32];
        let vault_uuid = "test-vault-123";

        // First call: generates new key
        let key1 = load_or_generate(dir.path(), vault_uuid, &encryption_key).unwrap();

        // Second call: loads existing key
        let key2 = load_or_generate(dir.path(), vault_uuid, &encryption_key).unwrap();

        assert_eq!(key1.to_bytes(), key2.to_bytes());
    }

    #[test]
    fn test_wrong_encryption_key_fails() {
        let dir = TempDir::new().unwrap();
        let encryption_key = [42u8; 32];
        let wrong_key = [99u8; 32];
        let vault_uuid = "test-vault-456";

        load_or_generate(dir.path(), vault_uuid, &encryption_key).unwrap();

        let result = load_or_generate(dir.path(), vault_uuid, &wrong_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_vaults_different_keys() {
        let dir = TempDir::new().unwrap();
        let encryption_key = [42u8; 32];

        let key_a = load_or_generate(dir.path(), "vault-a", &encryption_key).unwrap();
        let key_b = load_or_generate(dir.path(), "vault-b", &encryption_key).unwrap();

        assert_ne!(key_a.to_bytes(), key_b.to_bytes());
    }
}

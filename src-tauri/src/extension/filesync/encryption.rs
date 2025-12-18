// src-tauri/src/extension/filesync/encryption.rs
//!
//! File Encryption Layer
//!
//! Handles streaming encryption/decryption of files with chunking support.
//! Each chunk is encrypted with XChaCha20-Poly1305 with a unique nonce.
//!

use crate::extension::filesync::error::FileSyncError;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

/// Size of encryption key in bytes (256 bits)
pub const KEY_SIZE: usize = 32;

/// Size of nonce in bytes (192 bits for XChaCha20)
pub const NONCE_SIZE: usize = 24;

/// Size of authentication tag in bytes
pub const TAG_SIZE: usize = 16;

/// Default chunk size: 5MB
pub const DEFAULT_CHUNK_SIZE: usize = 5 * 1024 * 1024;

/// Chunk header: nonce (24 bytes)
pub const CHUNK_HEADER_SIZE: usize = NONCE_SIZE;

/// Overhead per chunk: nonce + auth tag
pub const CHUNK_OVERHEAD: usize = NONCE_SIZE + TAG_SIZE;

/// File encryption context
pub struct FileEncryption {
    key: [u8; KEY_SIZE],
    chunk_size: usize,
}

impl FileEncryption {
    /// Create a new FileEncryption with the given key
    pub fn new(key: [u8; KEY_SIZE]) -> Self {
        Self {
            key,
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    /// Create with custom chunk size
    pub fn with_chunk_size(key: [u8; KEY_SIZE], chunk_size: usize) -> Self {
        Self { key, chunk_size }
    }

    /// Generate a new random encryption key
    pub fn generate_key() -> [u8; KEY_SIZE] {
        let mut key = [0u8; KEY_SIZE];
        rand::rngs::OsRng.fill_bytes(&mut key);
        key
    }

    /// Generate a unique nonce for a chunk
    ///
    /// Uses: SHA256(file_id || chunk_index || random_salt)[0..24]
    fn generate_chunk_nonce(file_id: &str, chunk_index: u32) -> [u8; NONCE_SIZE] {
        let mut hasher = Sha256::new();
        hasher.update(file_id.as_bytes());
        hasher.update(chunk_index.to_le_bytes());

        // Add random salt for uniqueness
        let mut salt = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        hasher.update(&salt);

        let hash = hasher.finalize();
        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&hash[..NONCE_SIZE]);
        nonce
    }

    /// Encrypt a single chunk
    ///
    /// Returns: [nonce (24 bytes)][ciphertext + auth_tag]
    pub fn encrypt_chunk(
        &self,
        plaintext: &[u8],
        file_id: &str,
        chunk_index: u32,
    ) -> Result<Vec<u8>, FileSyncError> {
        let nonce = Self::generate_chunk_nonce(file_id, chunk_index);
        let cipher = XChaCha20Poly1305::new((&self.key).into());
        let xnonce = XNonce::from_slice(&nonce);

        let ciphertext = cipher
            .encrypt(xnonce, plaintext)
            .map_err(|_| FileSyncError::EncryptionError {
                reason: "Chunk encryption failed".to_string(),
            })?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        result.extend_from_slice(&nonce);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt a single chunk
    ///
    /// Expects: [nonce (24 bytes)][ciphertext + auth_tag]
    pub fn decrypt_chunk(&self, encrypted: &[u8]) -> Result<Vec<u8>, FileSyncError> {
        if encrypted.len() < CHUNK_OVERHEAD {
            return Err(FileSyncError::DecryptionError {
                reason: "Encrypted chunk too short".to_string(),
            });
        }

        let nonce = &encrypted[..NONCE_SIZE];
        let ciphertext = &encrypted[NONCE_SIZE..];

        let cipher = XChaCha20Poly1305::new((&self.key).into());
        let xnonce = XNonce::from_slice(nonce);

        cipher
            .decrypt(xnonce, ciphertext)
            .map_err(|_| FileSyncError::DecryptionError {
                reason: "Chunk decryption failed - data may be corrupted or key is wrong"
                    .to_string(),
            })
    }

    /// Encrypt a file from reader to writer
    ///
    /// Returns: (total_chunks, content_hash)
    pub fn encrypt_file<R: Read, W: Write>(
        &self,
        mut reader: R,
        mut writer: W,
        file_id: &str,
    ) -> Result<(u32, String), FileSyncError> {
        let mut chunk_index: u32 = 0;
        let mut buffer = vec![0u8; self.chunk_size];
        let mut hasher = Sha256::new();

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            // Update content hash with plaintext
            hasher.update(&buffer[..bytes_read]);

            // Encrypt chunk
            let encrypted = self.encrypt_chunk(&buffer[..bytes_read], file_id, chunk_index)?;

            // Write encrypted chunk with length prefix (4 bytes, little-endian)
            let chunk_len = encrypted.len() as u32;
            writer.write_all(&chunk_len.to_le_bytes())?;
            writer.write_all(&encrypted)?;

            chunk_index += 1;
        }

        writer.flush()?;

        let content_hash = hex::encode(hasher.finalize());
        Ok((chunk_index, content_hash))
    }

    /// Decrypt a file from reader to writer
    ///
    /// Returns: content_hash of decrypted data
    pub fn decrypt_file<R: Read, W: Write>(
        &self,
        mut reader: R,
        mut writer: W,
    ) -> Result<String, FileSyncError> {
        let mut hasher = Sha256::new();
        let mut len_buffer = [0u8; 4];

        loop {
            // Read chunk length
            match reader.read_exact(&mut len_buffer) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            let chunk_len = u32::from_le_bytes(len_buffer) as usize;
            if chunk_len == 0 {
                break;
            }

            // Read encrypted chunk
            let mut encrypted = vec![0u8; chunk_len];
            reader.read_exact(&mut encrypted)?;

            // Decrypt
            let plaintext = self.decrypt_chunk(&encrypted)?;

            // Update hash and write
            hasher.update(&plaintext);
            writer.write_all(&plaintext)?;
        }

        writer.flush()?;

        let content_hash = hex::encode(hasher.finalize());
        Ok(content_hash)
    }

    /// Calculate content hash without encryption (for comparison)
    pub fn hash_file<R: Read>(mut reader: R) -> Result<String, FileSyncError> {
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(hex::encode(hasher.finalize()))
    }
}

/// Wrapped key structure for storing encrypted keys
#[derive(Debug, Clone)]
pub struct WrappedKey {
    pub nonce: [u8; NONCE_SIZE],
    pub ciphertext: Vec<u8>,
}

impl WrappedKey {
    /// Serialize to bytes: [nonce][ciphertext]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(NONCE_SIZE + self.ciphertext.len());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FileSyncError> {
        if bytes.len() < NONCE_SIZE + TAG_SIZE {
            return Err(FileSyncError::DecryptionError {
                reason: "Wrapped key too short".to_string(),
            });
        }

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&bytes[..NONCE_SIZE]);
        let ciphertext = bytes[NONCE_SIZE..].to_vec();

        Ok(Self { nonce, ciphertext })
    }
}

/// Wrap (encrypt) a key with another key
pub fn wrap_key(
    key_to_wrap: &[u8; KEY_SIZE],
    wrapping_key: &[u8; KEY_SIZE],
) -> Result<WrappedKey, FileSyncError> {
    let mut nonce = [0u8; NONCE_SIZE];
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let cipher = XChaCha20Poly1305::new(wrapping_key.into());
    let xnonce = XNonce::from_slice(&nonce);

    let ciphertext = cipher
        .encrypt(xnonce, key_to_wrap.as_slice())
        .map_err(|_| FileSyncError::EncryptionError {
            reason: "Key wrapping failed".to_string(),
        })?;

    Ok(WrappedKey { nonce, ciphertext })
}

/// Unwrap (decrypt) a key with another key
pub fn unwrap_key(
    wrapped: &WrappedKey,
    wrapping_key: &[u8; KEY_SIZE],
) -> Result<[u8; KEY_SIZE], FileSyncError> {
    let cipher = XChaCha20Poly1305::new(wrapping_key.into());
    let xnonce = XNonce::from_slice(&wrapped.nonce);

    let plaintext = cipher
        .decrypt(xnonce, wrapped.ciphertext.as_slice())
        .map_err(|_| FileSyncError::DecryptionError {
            reason: "Key unwrapping failed".to_string(),
        })?;

    if plaintext.len() != KEY_SIZE {
        return Err(FileSyncError::DecryptionError {
            reason: "Unwrapped key has wrong size".to_string(),
        });
    }

    let mut key = [0u8; KEY_SIZE];
    key.copy_from_slice(&plaintext);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ============================================================================
    // Basic Chunk Encryption Tests
    // ============================================================================

    #[test]
    fn test_encrypt_decrypt_chunk() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let plaintext = b"Hello, World!";
        let encrypted = enc.encrypt_chunk(plaintext, "test-file", 0).unwrap();
        let decrypted = enc.decrypt_chunk(&encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_chunk_empty_data() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let plaintext = b"";
        let encrypted = enc.encrypt_chunk(plaintext, "test-file", 0).unwrap();
        let decrypted = enc.decrypt_chunk(&encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_chunk_large_data() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        // 1MB of data
        let plaintext: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
        let encrypted = enc.encrypt_chunk(&plaintext, "test-file", 0).unwrap();
        let decrypted = enc.decrypt_chunk(&encrypted).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_encrypt_chunk_binary_data() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        // Binary data with all byte values
        let plaintext: Vec<u8> = (0..=255).collect();
        let encrypted = enc.encrypt_chunk(&plaintext, "test-file", 0).unwrap();
        let decrypted = enc.decrypt_chunk(&encrypted).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_encrypted_chunk_has_correct_overhead() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let plaintext = b"Test data";
        let encrypted = enc.encrypt_chunk(plaintext, "test-file", 0).unwrap();

        // Encrypted size = nonce (24) + ciphertext (same as plaintext) + auth tag (16)
        assert_eq!(encrypted.len(), plaintext.len() + CHUNK_OVERHEAD);
    }

    #[test]
    fn test_different_chunks_different_nonces() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let plaintext = b"Same data";
        let encrypted1 = enc.encrypt_chunk(plaintext, "test-file", 0).unwrap();
        let encrypted2 = enc.encrypt_chunk(plaintext, "test-file", 1).unwrap();
        let encrypted3 = enc.encrypt_chunk(plaintext, "other-file", 0).unwrap();

        // Nonces (first 24 bytes) should be different
        assert_ne!(&encrypted1[..NONCE_SIZE], &encrypted2[..NONCE_SIZE]);
        assert_ne!(&encrypted1[..NONCE_SIZE], &encrypted3[..NONCE_SIZE]);
        assert_ne!(&encrypted2[..NONCE_SIZE], &encrypted3[..NONCE_SIZE]);
    }

    #[test]
    fn test_same_chunk_encrypted_twice_different_results() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let plaintext = b"Test data";
        let encrypted1 = enc.encrypt_chunk(plaintext, "test-file", 0).unwrap();
        let encrypted2 = enc.encrypt_chunk(plaintext, "test-file", 0).unwrap();

        // Same plaintext encrypted twice should give different ciphertext (due to random salt in nonce)
        assert_ne!(encrypted1, encrypted2);

        // But both should decrypt to the same plaintext
        assert_eq!(
            enc.decrypt_chunk(&encrypted1).unwrap(),
            enc.decrypt_chunk(&encrypted2).unwrap()
        );
    }

    // ============================================================================
    // Decryption Error Tests
    // ============================================================================

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let key1 = FileEncryption::generate_key();
        let key2 = FileEncryption::generate_key();
        let enc1 = FileEncryption::new(key1);
        let enc2 = FileEncryption::new(key2);

        let plaintext = b"Secret data";
        let encrypted = enc1.encrypt_chunk(plaintext, "test-file", 0).unwrap();

        let result = enc2.decrypt_chunk(&encrypted);
        assert!(result.is_err());
        match result {
            Err(FileSyncError::DecryptionError { reason }) => {
                assert!(reason.contains("corrupted") || reason.contains("wrong"));
            }
            _ => panic!("Expected DecryptionError"),
        }
    }

    #[test]
    fn test_decrypt_truncated_data_fails() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let plaintext = b"Test data";
        let encrypted = enc.encrypt_chunk(plaintext, "test-file", 0).unwrap();

        // Truncate the encrypted data
        let truncated = &encrypted[..encrypted.len() - 5];
        let result = enc.decrypt_chunk(truncated);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_corrupted_data_fails() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let plaintext = b"Test data";
        let mut encrypted = enc.encrypt_chunk(plaintext, "test-file", 0).unwrap();

        // Corrupt one byte in the ciphertext
        let corrupt_pos = NONCE_SIZE + 5;
        encrypted[corrupt_pos] ^= 0xFF;

        let result = enc.decrypt_chunk(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_corrupted_nonce_fails() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let plaintext = b"Test data";
        let mut encrypted = enc.encrypt_chunk(plaintext, "test-file", 0).unwrap();

        // Corrupt the nonce
        encrypted[0] ^= 0xFF;

        let result = enc.decrypt_chunk(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_too_short_data_fails() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        // Data shorter than minimum (nonce + tag)
        let short_data = vec![0u8; CHUNK_OVERHEAD - 1];
        let result = enc.decrypt_chunk(&short_data);
        assert!(result.is_err());
        match result {
            Err(FileSyncError::DecryptionError { reason }) => {
                assert!(reason.contains("too short"));
            }
            _ => panic!("Expected DecryptionError"),
        }
    }

    // ============================================================================
    // File Encryption Tests
    // ============================================================================

    #[test]
    fn test_encrypt_decrypt_file() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::with_chunk_size(key, 1024); // Small chunks for testing

        let original = b"This is a test file with some content that spans multiple chunks when using small chunk size.";

        let mut encrypted = Vec::new();
        let (chunks, hash1) = enc
            .encrypt_file(Cursor::new(original), &mut encrypted, "test-file")
            .unwrap();

        assert!(chunks > 0);

        let mut decrypted = Vec::new();
        let hash2 = enc
            .decrypt_file(Cursor::new(&encrypted), &mut decrypted)
            .unwrap();

        assert_eq!(original.as_slice(), decrypted.as_slice());
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_encrypt_decrypt_empty_file() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let original = b"";

        let mut encrypted = Vec::new();
        let (chunks, hash1) = enc
            .encrypt_file(Cursor::new(original), &mut encrypted, "test-file")
            .unwrap();

        assert_eq!(chunks, 0);

        let mut decrypted = Vec::new();
        let hash2 = enc
            .decrypt_file(Cursor::new(&encrypted), &mut decrypted)
            .unwrap();

        assert_eq!(original.as_slice(), decrypted.as_slice());
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_encrypt_decrypt_exact_chunk_size() {
        let key = FileEncryption::generate_key();
        let chunk_size = 1024;
        let enc = FileEncryption::with_chunk_size(key, chunk_size);

        // Exactly one chunk
        let original: Vec<u8> = (0..chunk_size).map(|i| (i % 256) as u8).collect();

        let mut encrypted = Vec::new();
        let (chunks, hash1) = enc
            .encrypt_file(Cursor::new(&original), &mut encrypted, "test-file")
            .unwrap();

        assert_eq!(chunks, 1);

        let mut decrypted = Vec::new();
        let hash2 = enc
            .decrypt_file(Cursor::new(&encrypted), &mut decrypted)
            .unwrap();

        assert_eq!(original, decrypted);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_encrypt_decrypt_multiple_chunks() {
        let key = FileEncryption::generate_key();
        let chunk_size = 1024;
        let enc = FileEncryption::with_chunk_size(key, chunk_size);

        // Exactly 5 chunks
        let original: Vec<u8> = (0..chunk_size * 5).map(|i| (i % 256) as u8).collect();

        let mut encrypted = Vec::new();
        let (chunks, hash1) = enc
            .encrypt_file(Cursor::new(&original), &mut encrypted, "test-file")
            .unwrap();

        assert_eq!(chunks, 5);

        let mut decrypted = Vec::new();
        let hash2 = enc
            .decrypt_file(Cursor::new(&encrypted), &mut decrypted)
            .unwrap();

        assert_eq!(original, decrypted);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_encrypt_decrypt_partial_last_chunk() {
        let key = FileEncryption::generate_key();
        let chunk_size = 1024;
        let enc = FileEncryption::with_chunk_size(key, chunk_size);

        // 3.5 chunks worth of data
        let original: Vec<u8> = (0..chunk_size * 3 + chunk_size / 2)
            .map(|i| (i % 256) as u8)
            .collect();

        let mut encrypted = Vec::new();
        let (chunks, hash1) = enc
            .encrypt_file(Cursor::new(&original), &mut encrypted, "test-file")
            .unwrap();

        assert_eq!(chunks, 4);

        let mut decrypted = Vec::new();
        let hash2 = enc
            .decrypt_file(Cursor::new(&encrypted), &mut decrypted)
            .unwrap();

        assert_eq!(original, decrypted);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_encrypt_decrypt_large_file() {
        let key = FileEncryption::generate_key();
        let chunk_size = 64 * 1024; // 64KB chunks
        let enc = FileEncryption::with_chunk_size(key, chunk_size);

        // 10MB file
        let original: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 256) as u8).collect();

        let mut encrypted = Vec::new();
        let (chunks, hash1) = enc
            .encrypt_file(Cursor::new(&original), &mut encrypted, "large-file")
            .unwrap();

        assert!(chunks > 0);

        let mut decrypted = Vec::new();
        let hash2 = enc
            .decrypt_file(Cursor::new(&encrypted), &mut decrypted)
            .unwrap();

        assert_eq!(original.len(), decrypted.len());
        assert_eq!(original, decrypted);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_content_hash_consistency() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let original = b"Test content for hashing";

        // Calculate hash directly
        let direct_hash = FileEncryption::hash_file(Cursor::new(original)).unwrap();

        // Encrypt and get hash
        let mut encrypted = Vec::new();
        let (_, encrypt_hash) = enc
            .encrypt_file(Cursor::new(original), &mut encrypted, "test-file")
            .unwrap();

        // Decrypt and get hash
        let mut decrypted = Vec::new();
        let decrypt_hash = enc
            .decrypt_file(Cursor::new(&encrypted), &mut decrypted)
            .unwrap();

        // All hashes should match
        assert_eq!(direct_hash, encrypt_hash);
        assert_eq!(encrypt_hash, decrypt_hash);
    }

    // ============================================================================
    // Key Wrapping Tests
    // ============================================================================

    #[test]
    fn test_wrap_unwrap_key() {
        let key_to_wrap = FileEncryption::generate_key();
        let wrapping_key = FileEncryption::generate_key();

        let wrapped = wrap_key(&key_to_wrap, &wrapping_key).unwrap();
        let unwrapped = unwrap_key(&wrapped, &wrapping_key).unwrap();

        assert_eq!(key_to_wrap, unwrapped);
    }

    #[test]
    fn test_wrapped_key_serialization() {
        let key_to_wrap = FileEncryption::generate_key();
        let wrapping_key = FileEncryption::generate_key();

        let wrapped = wrap_key(&key_to_wrap, &wrapping_key).unwrap();
        let bytes = wrapped.to_bytes();
        let restored = WrappedKey::from_bytes(&bytes).unwrap();
        let unwrapped = unwrap_key(&restored, &wrapping_key).unwrap();

        assert_eq!(key_to_wrap, unwrapped);
    }

    #[test]
    fn test_unwrap_with_wrong_key_fails() {
        let key_to_wrap = FileEncryption::generate_key();
        let wrapping_key1 = FileEncryption::generate_key();
        let wrapping_key2 = FileEncryption::generate_key();

        let wrapped = wrap_key(&key_to_wrap, &wrapping_key1).unwrap();
        let result = unwrap_key(&wrapped, &wrapping_key2);

        assert!(result.is_err());
        match result {
            Err(FileSyncError::DecryptionError { .. }) => {}
            _ => panic!("Expected DecryptionError"),
        }
    }

    #[test]
    fn test_unwrap_corrupted_wrapped_key_fails() {
        let key_to_wrap = FileEncryption::generate_key();
        let wrapping_key = FileEncryption::generate_key();

        let mut wrapped = wrap_key(&key_to_wrap, &wrapping_key).unwrap();
        wrapped.ciphertext[0] ^= 0xFF;

        let result = unwrap_key(&wrapped, &wrapping_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrapped_key_from_short_bytes_fails() {
        let short_bytes = vec![0u8; NONCE_SIZE + TAG_SIZE - 1];
        let result = WrappedKey::from_bytes(&short_bytes);
        assert!(result.is_err());
    }

    // ============================================================================
    // Key Hierarchy Tests (Master Key -> Space Key -> File Key)
    // ============================================================================

    #[test]
    fn test_full_key_hierarchy() {
        // Simulate the full key hierarchy:
        // Master Key -> wraps -> Space Key -> wraps -> File Key

        let master_key = FileEncryption::generate_key();
        let space_key = FileEncryption::generate_key();
        let file_key = FileEncryption::generate_key();

        // Wrap space key with master key
        let wrapped_space_key = wrap_key(&space_key, &master_key).unwrap();

        // Wrap file key with space key
        let wrapped_file_key = wrap_key(&file_key, &space_key).unwrap();

        // Encrypt some file content with the file key
        let enc = FileEncryption::new(file_key);
        let plaintext = b"Secret file content";
        let encrypted_content = enc.encrypt_chunk(plaintext, "file-123", 0).unwrap();

        // Now simulate accessing the file:
        // 1. Unwrap space key with master key
        let recovered_space_key = unwrap_key(&wrapped_space_key, &master_key).unwrap();
        assert_eq!(space_key, recovered_space_key);

        // 2. Unwrap file key with space key
        let recovered_file_key = unwrap_key(&wrapped_file_key, &recovered_space_key).unwrap();
        assert_eq!(file_key, recovered_file_key);

        // 3. Decrypt file content
        let dec = FileEncryption::new(recovered_file_key);
        let decrypted = dec.decrypt_chunk(&encrypted_content).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_shared_space_scenario() {
        // Simulate shared space:
        // User A creates a space with their master key
        // User B gets the space key wrapped with their master key
        // Both can access files in the shared space

        // User A's setup
        let user_a_master_key = FileEncryption::generate_key();
        let shared_space_key = FileEncryption::generate_key();
        let wrapped_for_a = wrap_key(&shared_space_key, &user_a_master_key).unwrap();

        // User B's setup (different master key)
        let user_b_master_key = FileEncryption::generate_key();
        // Space key is wrapped with User B's master key when they're invited
        let wrapped_for_b = wrap_key(&shared_space_key, &user_b_master_key).unwrap();

        // Create a file in the shared space
        let file_key = FileEncryption::generate_key();
        let wrapped_file_key = wrap_key(&file_key, &shared_space_key).unwrap();

        let enc = FileEncryption::new(file_key);
        let secret_content = b"Shared secret document";
        let encrypted = enc.encrypt_chunk(secret_content, "shared-doc", 0).unwrap();

        // User A accesses the file
        let a_space_key = unwrap_key(&wrapped_for_a, &user_a_master_key).unwrap();
        let a_file_key = unwrap_key(&wrapped_file_key, &a_space_key).unwrap();
        let dec_a = FileEncryption::new(a_file_key);
        let decrypted_a = dec_a.decrypt_chunk(&encrypted).unwrap();
        assert_eq!(secret_content.as_slice(), decrypted_a.as_slice());

        // User B accesses the same file
        let b_space_key = unwrap_key(&wrapped_for_b, &user_b_master_key).unwrap();
        let b_file_key = unwrap_key(&wrapped_file_key, &b_space_key).unwrap();
        let dec_b = FileEncryption::new(b_file_key);
        let decrypted_b = dec_b.decrypt_chunk(&encrypted).unwrap();
        assert_eq!(secret_content.as_slice(), decrypted_b.as_slice());

        // Both should have the same space key and file key
        assert_eq!(a_space_key, b_space_key);
        assert_eq!(a_file_key, b_file_key);
    }

    #[test]
    fn test_key_rotation_scenario() {
        // Simulate key rotation for a space:
        // 1. Create space with original key
        // 2. Create some files
        // 3. Rotate space key
        // 4. Re-wrap file keys with new space key
        // 5. Old files should still be accessible

        let master_key = FileEncryption::generate_key();
        let original_space_key = FileEncryption::generate_key();

        // Create a file with the original space key
        let file_key = FileEncryption::generate_key();
        let wrapped_file_key_v1 = wrap_key(&file_key, &original_space_key).unwrap();

        let enc = FileEncryption::new(file_key);
        let content = b"Important document";
        let encrypted = enc.encrypt_chunk(content, "doc-1", 0).unwrap();

        // Rotate to new space key
        let new_space_key = FileEncryption::generate_key();

        // Re-wrap file key with new space key
        // First unwrap with old key
        let recovered_file_key = unwrap_key(&wrapped_file_key_v1, &original_space_key).unwrap();
        // Then wrap with new key
        let wrapped_file_key_v2 = wrap_key(&recovered_file_key, &new_space_key).unwrap();

        // Wrap new space key with master key (replacing old wrapped space key)
        let wrapped_new_space_key = wrap_key(&new_space_key, &master_key).unwrap();

        // Access file with new hierarchy
        let space_key = unwrap_key(&wrapped_new_space_key, &master_key).unwrap();
        assert_eq!(space_key, new_space_key);

        let file_key_recovered = unwrap_key(&wrapped_file_key_v2, &space_key).unwrap();
        assert_eq!(file_key_recovered, file_key);

        let dec = FileEncryption::new(file_key_recovered);
        let decrypted = dec.decrypt_chunk(&encrypted).unwrap();
        assert_eq!(content.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_revoke_access_scenario() {
        // Simulate revoking access from a user:
        // User loses access to space key, cannot decrypt files

        let user_a_master_key = FileEncryption::generate_key();
        let user_b_master_key = FileEncryption::generate_key();
        let space_key = FileEncryption::generate_key();

        let wrapped_for_a = wrap_key(&space_key, &user_a_master_key).unwrap();
        let wrapped_for_b = wrap_key(&space_key, &user_b_master_key).unwrap();

        let file_key = FileEncryption::generate_key();
        let wrapped_file_key = wrap_key(&file_key, &space_key).unwrap();

        let enc = FileEncryption::new(file_key);
        let content = b"Confidential";
        let encrypted = enc.encrypt_chunk(content, "file", 0).unwrap();

        // Both users can access initially
        let a_space_key = unwrap_key(&wrapped_for_a, &user_a_master_key).unwrap();
        let a_file_key = unwrap_key(&wrapped_file_key, &a_space_key).unwrap();
        let dec_a = FileEncryption::new(a_file_key);
        assert_eq!(
            content.as_slice(),
            dec_a.decrypt_chunk(&encrypted).unwrap().as_slice()
        );

        // Simulate revoking User B's access by rotating space key
        let new_space_key = FileEncryption::generate_key();
        let new_wrapped_for_a = wrap_key(&new_space_key, &user_a_master_key).unwrap();
        // Note: We don't create new_wrapped_for_b - User B is not invited to new space key

        // Re-wrap file key with new space key
        let new_wrapped_file_key = wrap_key(&file_key, &new_space_key).unwrap();

        // User A can still access with new keys
        let a_new_space_key = unwrap_key(&new_wrapped_for_a, &user_a_master_key).unwrap();
        let a_new_file_key = unwrap_key(&new_wrapped_file_key, &a_new_space_key).unwrap();
        let dec_a_new = FileEncryption::new(a_new_file_key);
        assert_eq!(
            content.as_slice(),
            dec_a_new.decrypt_chunk(&encrypted).unwrap().as_slice()
        );

        // User B cannot access with old keys (wrapped file key uses new space key)
        let b_old_space_key = unwrap_key(&wrapped_for_b, &user_b_master_key).unwrap();
        let result = unwrap_key(&new_wrapped_file_key, &b_old_space_key);
        assert!(result.is_err()); // User B cannot unwrap file key with old space key
    }

    // ============================================================================
    // Edge Cases and Security Tests
    // ============================================================================

    #[test]
    fn test_key_generation_uniqueness() {
        let mut keys = Vec::new();
        for _ in 0..100 {
            keys.push(FileEncryption::generate_key());
        }

        // All keys should be unique
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(keys[i], keys[j], "Generated keys should be unique");
            }
        }
    }

    #[test]
    fn test_key_not_all_zeros() {
        for _ in 0..100 {
            let key = FileEncryption::generate_key();
            assert_ne!(key, [0u8; KEY_SIZE], "Generated key should not be all zeros");
        }
    }

    #[test]
    fn test_wrapped_key_different_each_time() {
        let key_to_wrap = FileEncryption::generate_key();
        let wrapping_key = FileEncryption::generate_key();

        let wrapped1 = wrap_key(&key_to_wrap, &wrapping_key).unwrap();
        let wrapped2 = wrap_key(&key_to_wrap, &wrapping_key).unwrap();

        // Same key wrapped twice should produce different results (different nonces)
        assert_ne!(wrapped1.to_bytes(), wrapped2.to_bytes());

        // But both should unwrap to the same key
        let unwrapped1 = unwrap_key(&wrapped1, &wrapping_key).unwrap();
        let unwrapped2 = unwrap_key(&wrapped2, &wrapping_key).unwrap();
        assert_eq!(unwrapped1, unwrapped2);
        assert_eq!(unwrapped1, key_to_wrap);
    }

    #[test]
    fn test_deterministic_content_hash() {
        let original = b"Test content";

        let hash1 = FileEncryption::hash_file(Cursor::new(original)).unwrap();
        let hash2 = FileEncryption::hash_file(Cursor::new(original)).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_content_different_hash() {
        let content1 = b"Content A";
        let content2 = b"Content B";

        let hash1 = FileEncryption::hash_file(Cursor::new(content1)).unwrap();
        let hash2 = FileEncryption::hash_file(Cursor::new(content2)).unwrap();

        assert_ne!(hash1, hash2);
    }

    // ============================================================================
    // Cross-Platform Compatibility Tests
    // ============================================================================

    #[test]
    fn test_encrypted_data_format() {
        // Verify the encrypted chunk format is correct
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        let plaintext = b"Test";
        let encrypted = enc.encrypt_chunk(plaintext, "test", 0).unwrap();

        // Format: [nonce: 24 bytes][ciphertext: len(plaintext) bytes][auth_tag: 16 bytes]
        assert_eq!(encrypted.len(), NONCE_SIZE + plaintext.len() + TAG_SIZE);

        // Nonce should be at the beginning
        let nonce = &encrypted[..NONCE_SIZE];
        assert_eq!(nonce.len(), 24);

        // Ciphertext + tag at the end
        let ciphertext_with_tag = &encrypted[NONCE_SIZE..];
        assert_eq!(ciphertext_with_tag.len(), plaintext.len() + TAG_SIZE);
    }

    #[test]
    fn test_wrapped_key_format() {
        let key_to_wrap = FileEncryption::generate_key();
        let wrapping_key = FileEncryption::generate_key();

        let wrapped = wrap_key(&key_to_wrap, &wrapping_key).unwrap();
        let bytes = wrapped.to_bytes();

        // Format: [nonce: 24 bytes][ciphertext: 32 bytes][auth_tag: 16 bytes]
        assert_eq!(bytes.len(), NONCE_SIZE + KEY_SIZE + TAG_SIZE);
    }

    #[test]
    fn test_file_format_with_length_prefix() {
        let key = FileEncryption::generate_key();
        let chunk_size = 1024;
        let enc = FileEncryption::with_chunk_size(key, chunk_size);

        // Create data that spans 3 chunks
        let original: Vec<u8> = (0..chunk_size * 2 + 500).map(|i| (i % 256) as u8).collect();

        let mut encrypted = Vec::new();
        let (chunks, _) = enc
            .encrypt_file(Cursor::new(&original), &mut encrypted, "test")
            .unwrap();

        assert_eq!(chunks, 3);

        // Verify we can parse the length prefixes
        let mut cursor = Cursor::new(&encrypted);
        for i in 0..chunks {
            let mut len_buf = [0u8; 4];
            cursor.read_exact(&mut len_buf).unwrap();
            let chunk_len = u32::from_le_bytes(len_buf) as usize;

            // Each chunk should have correct overhead
            let expected_plaintext_len = if i < 2 { chunk_size } else { 500 };
            assert_eq!(chunk_len, expected_plaintext_len + CHUNK_OVERHEAD);

            // Skip the chunk data
            let mut chunk_data = vec![0u8; chunk_len];
            cursor.read_exact(&mut chunk_data).unwrap();
        }
    }

    // ============================================================================
    // Performance Sanity Tests
    // ============================================================================

    #[test]
    fn test_many_chunks_sequential() {
        let key = FileEncryption::generate_key();
        let enc = FileEncryption::new(key);

        // Encrypt and decrypt many chunks sequentially
        for i in 0..1000 {
            let plaintext = format!("Chunk {}", i);
            let encrypted = enc
                .encrypt_chunk(plaintext.as_bytes(), "test-file", i)
                .unwrap();
            let decrypted = enc.decrypt_chunk(&encrypted).unwrap();
            assert_eq!(plaintext.as_bytes(), decrypted.as_slice());
        }
    }

    #[test]
    fn test_many_key_wraps() {
        let wrapping_key = FileEncryption::generate_key();

        for _ in 0..100 {
            let key = FileEncryption::generate_key();
            let wrapped = wrap_key(&key, &wrapping_key).unwrap();
            let unwrapped = unwrap_key(&wrapped, &wrapping_key).unwrap();
            assert_eq!(key, unwrapped);
        }
    }
}

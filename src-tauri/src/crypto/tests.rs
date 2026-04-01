//! Tests for identity-based encryption (Ed25519 → X25519 ECDH + HKDF-SHA256 + AES-256-GCM)
//!
//! Coverage:
//! - ASN.1 SPKI/PKCS8 parsing and wrapping
//! - Ed25519 → X25519 key conversion (public + private)
//! - Encrypt/decrypt roundtrip (normal, empty, large, unicode)
//! - Non-determinism (each encryption produces unique output)
//! - Output format validation (nonce, salt, SPKI lengths)
//! - Wrong key rejection
//! - Tamper detection (ciphertext, nonce, salt, ephemeral key)
//! - Invalid input handling (bad base64, wrong key types, truncated data)

#[cfg(test)]
mod tests {
    use crate::crypto::*;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use ed25519_dalek::SigningKey;
    use x25519_dalek::{PublicKey, StaticSecret};

    /// Generate a deterministic Ed25519 keypair from a seed byte,
    /// return (SPKI public, PKCS8 private) as Base64
    fn generate_test_identity(seed_byte: u8) -> (String, String) {
        let seed = [seed_byte; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();

        let mut spki = Vec::with_capacity(44);
        spki.extend_from_slice(&ED25519_SPKI_PREFIX);
        spki.extend_from_slice(verifying_key.as_bytes());

        let mut pkcs8 = Vec::with_capacity(48);
        pkcs8.extend_from_slice(&ED25519_PKCS8_PREFIX);
        pkcs8.extend_from_slice(&seed);

        (BASE64.encode(&spki), BASE64.encode(&pkcs8))
    }

    // ═══════════════════════════════════════════════════════════════════
    // ASN.1 SPKI / PKCS8 parsing
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn extract_ed25519_public_key_from_valid_spki() {
        let (spki_b64, _) = generate_test_identity(0x42);
        let spki = BASE64.decode(&spki_b64).unwrap();
        let raw = extract_ed25519_public_key_from_spki(&spki).unwrap();
        assert_eq!(raw.len(), 32);
    }

    #[test]
    fn extract_ed25519_seed_from_valid_pkcs8() {
        let (_, pkcs8_b64) = generate_test_identity(0x42);
        let pkcs8 = BASE64.decode(&pkcs8_b64).unwrap();
        let seed = extract_ed25519_seed_from_pkcs8(&pkcs8).unwrap();
        assert_eq!(seed, [0x42; 32]);
    }

    #[test]
    fn reject_wrong_length_spki() {
        let too_short = [0u8; 30];
        assert!(extract_ed25519_public_key_from_spki(&too_short).is_err());
    }

    #[test]
    fn reject_x25519_oid_as_ed25519_spki() {
        let mut x25519_spki = [0u8; 44];
        x25519_spki[..12].copy_from_slice(&X25519_SPKI_PREFIX);
        assert!(extract_ed25519_public_key_from_spki(&x25519_spki).is_err());
    }

    #[test]
    fn reject_wrong_length_pkcs8() {
        let too_short = [0u8; 32];
        assert!(extract_ed25519_seed_from_pkcs8(&too_short).is_err());
    }

    #[test]
    fn reject_corrupted_oid_pkcs8() {
        let mut bad_pkcs8 = [0u8; 48];
        bad_pkcs8[..16].copy_from_slice(&ED25519_PKCS8_PREFIX);
        bad_pkcs8[11] = 0xFF;
        assert!(extract_ed25519_seed_from_pkcs8(&bad_pkcs8).is_err());
    }

    #[test]
    fn x25519_spki_wrap_unwrap_roundtrip() {
        let raw = [0xAB; 32];
        let spki = wrap_x25519_public_key_as_spki(&raw);
        assert_eq!(spki.len(), 44);
        let extracted = extract_x25519_public_key_from_spki(&spki).unwrap();
        assert_eq!(extracted, raw);
    }

    #[test]
    fn reject_ed25519_spki_as_x25519() {
        let (spki_b64, _) = generate_test_identity(0x01);
        let spki = BASE64.decode(&spki_b64).unwrap();
        assert!(extract_x25519_public_key_from_spki(&spki).is_err());
    }

    // ═══════════════════════════════════════════════════════════════════
    // Ed25519 → X25519 key conversion
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn public_key_conversion_is_deterministic() {
        let seed = [0x42; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let raw_ed = *signing_key.verifying_key().as_bytes();

        let x1 = ed25519_public_to_x25519(&raw_ed).unwrap();
        let x2 = ed25519_public_to_x25519(&raw_ed).unwrap();
        assert_eq!(x1, x2);
        assert_ne!(x1, [0u8; 32]);
    }

    #[test]
    fn private_key_conversion_is_deterministic() {
        let seed = [0x42; 32];
        let x1 = ed25519_seed_to_x25519(&seed);
        let x2 = ed25519_seed_to_x25519(&seed);
        assert_eq!(x1, x2);
        assert_ne!(x1, [0u8; 32]);
    }

    #[test]
    fn clamping_is_applied_correctly() {
        let seed = [0xFF; 32];
        let x = ed25519_seed_to_x25519(&seed);
        assert_eq!(x[0] & 0x07, 0, "bits 0,1,2 of first byte must be cleared");
        assert_eq!(x[31] & 0x80, 0, "bit 7 of last byte must be cleared");
        assert_eq!(x[31] & 0x40, 0x40, "bit 6 of last byte must be set");
    }

    #[test]
    fn different_seeds_produce_different_x25519_keys() {
        let x1 = ed25519_seed_to_x25519(&[0x01; 32]);
        let x2 = ed25519_seed_to_x25519(&[0x02; 32]);
        assert_ne!(x1, x2);
    }

    #[test]
    fn derived_public_and_private_keys_form_valid_ecdh_pair() {
        let seed = [0x42; 32];
        let signing_key = SigningKey::from_bytes(&seed);

        // Derive X25519 private from seed
        let x25519_sk_bytes = ed25519_seed_to_x25519(&seed);
        let x25519_sk = StaticSecret::from(x25519_sk_bytes);
        let x25519_pk_from_private = PublicKey::from(&x25519_sk);

        // Convert Ed25519 public to X25519 public
        let x25519_pk_from_ed25519 =
            ed25519_public_to_x25519(signing_key.verifying_key().as_bytes()).unwrap();

        assert_eq!(
            x25519_pk_from_private.as_bytes(),
            &x25519_pk_from_ed25519,
            "public key derived from private must match public key converted from Ed25519"
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    // Encrypt / Decrypt roundtrip
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn roundtrip_basic() {
        let (pub_b64, priv_b64) = generate_test_identity(0x01);
        let plaintext = "Hello, Vault!";

        let sealed = encrypt_for_identity(BASE64.encode(plaintext.as_bytes()), pub_b64).unwrap();
        let decrypted_b64 = decrypt_for_identity(
            sealed.encrypted_data,
            sealed.nonce,
            sealed.salt,
            sealed.ephemeral_public_key,
            priv_b64,
        )
        .unwrap();

        let decrypted = BASE64.decode(&decrypted_b64).unwrap();
        assert_eq!(String::from_utf8(decrypted).unwrap(), plaintext);
    }

    #[test]
    fn roundtrip_empty_plaintext() {
        let (pub_b64, priv_b64) = generate_test_identity(0x02);

        let sealed = encrypt_for_identity(BASE64.encode(b""), pub_b64).unwrap();
        let decrypted_b64 = decrypt_for_identity(
            sealed.encrypted_data,
            sealed.nonce,
            sealed.salt,
            sealed.ephemeral_public_key,
            priv_b64,
        )
        .unwrap();

        assert_eq!(BASE64.decode(&decrypted_b64).unwrap(), b"");
    }

    #[test]
    fn roundtrip_large_plaintext() {
        let (pub_b64, priv_b64) = generate_test_identity(0x03);
        let plaintext = "A".repeat(100_000);

        let sealed =
            encrypt_for_identity(BASE64.encode(plaintext.as_bytes()), pub_b64).unwrap();
        let decrypted_b64 = decrypt_for_identity(
            sealed.encrypted_data,
            sealed.nonce,
            sealed.salt,
            sealed.ephemeral_public_key,
            priv_b64,
        )
        .unwrap();

        assert_eq!(BASE64.decode(&decrypted_b64).unwrap().len(), 100_000);
    }

    #[test]
    fn roundtrip_unicode() {
        let (pub_b64, priv_b64) = generate_test_identity(0x04);
        let plaintext = "Ünîcödé 🔐🗝️ Тест";

        let sealed =
            encrypt_for_identity(BASE64.encode(plaintext.as_bytes()), pub_b64).unwrap();
        let decrypted_b64 = decrypt_for_identity(
            sealed.encrypted_data,
            sealed.nonce,
            sealed.salt,
            sealed.ephemeral_public_key,
            priv_b64,
        )
        .unwrap();

        let decrypted = BASE64.decode(&decrypted_b64).unwrap();
        assert_eq!(String::from_utf8(decrypted).unwrap(), plaintext);
    }

    // ═══════════════════════════════════════════════════════════════════
    // Non-determinism: each encryption must produce unique output
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn each_encryption_produces_unique_output() {
        let (pub_b64, _) = generate_test_identity(0x05);
        let input = BASE64.encode(b"same input");

        let sealed1 = encrypt_for_identity(input.clone(), pub_b64.clone()).unwrap();
        let sealed2 = encrypt_for_identity(input, pub_b64).unwrap();

        assert_ne!(sealed1.ephemeral_public_key, sealed2.ephemeral_public_key);
        assert_ne!(sealed1.nonce, sealed2.nonce);
        assert_ne!(sealed1.salt, sealed2.salt);
        assert_ne!(sealed1.encrypted_data, sealed2.encrypted_data);
    }

    // ═══════════════════════════════════════════════════════════════════
    // Output format validation
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn sealed_data_has_correct_field_lengths() {
        let (pub_b64, _) = generate_test_identity(0x06);
        let sealed = encrypt_for_identity(BASE64.encode(b"test"), pub_b64).unwrap();

        let nonce = BASE64.decode(&sealed.nonce).unwrap();
        assert_eq!(nonce.len(), IV_LENGTH, "nonce must be 12 bytes");

        let salt = BASE64.decode(&sealed.salt).unwrap();
        assert_eq!(salt.len(), SALT_LENGTH, "salt must be 32 bytes");

        let eph_spki = BASE64.decode(&sealed.ephemeral_public_key).unwrap();
        assert_eq!(eph_spki.len(), 44, "ephemeral key must be 44 bytes (SPKI)");
        assert_eq!(
            &eph_spki[..12],
            &X25519_SPKI_PREFIX,
            "ephemeral key must have X25519 OID"
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    // Wrong key → decryption must fail
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn decrypt_with_wrong_identity_fails() {
        let (pub_b64, _) = generate_test_identity(0x0A);
        let (_, wrong_priv_b64) = generate_test_identity(0x0B);

        let sealed = encrypt_for_identity(BASE64.encode(b"secret"), pub_b64).unwrap();

        let result = decrypt_for_identity(
            sealed.encrypted_data,
            sealed.nonce,
            sealed.salt,
            sealed.ephemeral_public_key,
            wrong_priv_b64,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Decryption failed"));
    }

    // ═══════════════════════════════════════════════════════════════════
    // Tamper detection: any modification must cause decryption failure
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn tampered_ciphertext_detected() {
        let (pub_b64, priv_b64) = generate_test_identity(0x10);
        let sealed = encrypt_for_identity(BASE64.encode(b"secret"), pub_b64).unwrap();

        let mut ct = BASE64.decode(&sealed.encrypted_data).unwrap();
        ct[0] ^= 0xFF;

        let result = decrypt_for_identity(
            BASE64.encode(&ct),
            sealed.nonce,
            sealed.salt,
            sealed.ephemeral_public_key,
            priv_b64,
        );
        assert!(result.is_err());
    }

    #[test]
    fn tampered_nonce_detected() {
        let (pub_b64, priv_b64) = generate_test_identity(0x11);
        let sealed = encrypt_for_identity(BASE64.encode(b"secret"), pub_b64).unwrap();

        let mut nonce = BASE64.decode(&sealed.nonce).unwrap();
        nonce[0] ^= 0xFF;

        let result = decrypt_for_identity(
            sealed.encrypted_data,
            BASE64.encode(&nonce),
            sealed.salt,
            sealed.ephemeral_public_key,
            priv_b64,
        );
        assert!(result.is_err());
    }

    #[test]
    fn tampered_salt_detected() {
        let (pub_b64, priv_b64) = generate_test_identity(0x12);
        let sealed = encrypt_for_identity(BASE64.encode(b"secret"), pub_b64).unwrap();

        let mut salt = BASE64.decode(&sealed.salt).unwrap();
        salt[0] ^= 0xFF;

        let result = decrypt_for_identity(
            sealed.encrypted_data,
            sealed.nonce,
            BASE64.encode(&salt),
            sealed.ephemeral_public_key,
            priv_b64,
        );
        assert!(result.is_err());
    }

    #[test]
    fn tampered_ephemeral_key_detected() {
        let (pub_b64, priv_b64) = generate_test_identity(0x13);
        let sealed = encrypt_for_identity(BASE64.encode(b"secret"), pub_b64).unwrap();

        let mut eph = BASE64.decode(&sealed.ephemeral_public_key).unwrap();
        eph[20] ^= 0xFF; // flip byte in key material (after SPKI header)

        let result = decrypt_for_identity(
            sealed.encrypted_data,
            sealed.nonce,
            sealed.salt,
            BASE64.encode(&eph),
            priv_b64,
        );
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════════
    // Invalid input handling
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn encrypt_rejects_invalid_base64() {
        let result = encrypt_for_identity(
            "not-base64!!!".to_string(),
            "also-not-base64!!!".to_string(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn encrypt_rejects_x25519_key_as_identity_key() {
        let raw = [0x42; 32];
        let x25519_spki = wrap_x25519_public_key_as_spki(&raw);

        let result = encrypt_for_identity(BASE64.encode(b"test"), BASE64.encode(&x25519_spki));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("OID mismatch"));
    }

    #[test]
    fn encrypt_rejects_raw_key_without_spki_wrapper() {
        let raw_key = BASE64.encode(&[0x42; 32]);
        let result = encrypt_for_identity(BASE64.encode(b"test"), raw_key);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_rejects_truncated_nonce() {
        let (pub_b64, priv_b64) = generate_test_identity(0x20);
        let sealed = encrypt_for_identity(BASE64.encode(b"test"), pub_b64).unwrap();

        let result = decrypt_for_identity(
            sealed.encrypted_data,
            BASE64.encode(&[0u8; 6]),
            sealed.salt,
            sealed.ephemeral_public_key,
            priv_b64,
        );
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_rejects_ed25519_spki_as_ephemeral_key() {
        let (pub_b64, priv_b64) = generate_test_identity(0x21);
        let sealed = encrypt_for_identity(BASE64.encode(b"test"), pub_b64.clone()).unwrap();

        let result = decrypt_for_identity(
            sealed.encrypted_data,
            sealed.nonce,
            sealed.salt,
            pub_b64, // Ed25519 SPKI instead of X25519 SPKI
            priv_b64,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("OID mismatch"));
    }

    #[test]
    fn decrypt_rejects_raw_ephemeral_key_without_spki() {
        let (pub_b64, priv_b64) = generate_test_identity(0x22);
        let sealed = encrypt_for_identity(BASE64.encode(b"test"), pub_b64).unwrap();

        let result = decrypt_for_identity(
            sealed.encrypted_data,
            sealed.nonce,
            sealed.salt,
            BASE64.encode(&[0x42; 32]), // raw 32 bytes, no SPKI
            priv_b64,
        );
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_rejects_raw_private_key_without_pkcs8() {
        let (pub_b64, _) = generate_test_identity(0x23);
        let sealed = encrypt_for_identity(BASE64.encode(b"test"), pub_b64).unwrap();

        let result = decrypt_for_identity(
            sealed.encrypted_data,
            sealed.nonce,
            sealed.salt,
            sealed.ephemeral_public_key,
            BASE64.encode(&[0x42; 32]), // raw 32 bytes, no PKCS8
        );
        assert!(result.is_err());
    }
}

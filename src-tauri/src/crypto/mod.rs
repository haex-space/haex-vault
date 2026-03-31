use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::VerifyingKey;
use hkdf::Hkdf;
use sha2::{Digest, Sha256, Sha512};
use x25519_dalek::{PublicKey, StaticSecret};

const IV_LENGTH: usize = 12;
const SALT_LENGTH: usize = 32;
const HKDF_INFO: &[u8] = b"haex-vault-name-encryption";

// ── ASN.1 constants ─────────────────────────────────────────────────

// X25519 SPKI prefix: SEQUENCE(42) → SEQUENCE(5) → OID(1.3.101.110) → BITSTRING(33, 0 unused)
const X25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6e, 0x03, 0x21, 0x00,
];

// Ed25519 SPKI prefix: same structure, OID = 1.3.101.112
const ED25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

// Ed25519 PKCS8 prefix: SEQUENCE(46) → INTEGER(0) → SEQUENCE(5) → OID(1.3.101.112) → OCTETSTRING(34) → OCTETSTRING(32)
const ED25519_PKCS8_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
    0x20,
];

// ── ASN.1 helpers ───────────────────────────────────────────────────

fn extract_ed25519_public_key_from_spki(spki: &[u8]) -> Result<[u8; 32], String> {
    if spki.len() != 44 {
        return Err(format!(
            "Invalid Ed25519 SPKI length: expected 44, got {}",
            spki.len()
        ));
    }
    if spki[..12] != ED25519_SPKI_PREFIX {
        return Err("Not an Ed25519 SPKI key (OID mismatch)".to_string());
    }
    let mut raw = [0u8; 32];
    raw.copy_from_slice(&spki[12..]);
    Ok(raw)
}

fn extract_ed25519_seed_from_pkcs8(pkcs8: &[u8]) -> Result<[u8; 32], String> {
    if pkcs8.len() != 48 {
        return Err(format!(
            "Invalid Ed25519 PKCS8 length: expected 48, got {}",
            pkcs8.len()
        ));
    }
    if pkcs8[..16] != ED25519_PKCS8_PREFIX {
        return Err("Not an Ed25519 PKCS8 key (OID mismatch)".to_string());
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&pkcs8[16..]);
    Ok(seed)
}

fn extract_x25519_public_key_from_spki(spki: &[u8]) -> Result<[u8; 32], String> {
    if spki.len() != 44 {
        return Err(format!(
            "Invalid X25519 SPKI length: expected 44, got {}",
            spki.len()
        ));
    }
    if spki[..12] != X25519_SPKI_PREFIX {
        return Err("Not an X25519 SPKI key (OID mismatch)".to_string());
    }
    let mut raw = [0u8; 32];
    raw.copy_from_slice(&spki[12..]);
    Ok(raw)
}

fn wrap_x25519_public_key_as_spki(raw: &[u8; 32]) -> Vec<u8> {
    let mut spki = Vec::with_capacity(44);
    spki.extend_from_slice(&X25519_SPKI_PREFIX);
    spki.extend_from_slice(raw);
    spki
}

// ── Ed25519 → X25519 conversion ────────────────────────────────────

fn ed25519_public_to_x25519(ed25519_raw: &[u8; 32]) -> Result<[u8; 32], String> {
    let verifying_key = VerifyingKey::from_bytes(ed25519_raw)
        .map_err(|e| format!("Invalid Ed25519 public key: {e}"))?;
    Ok(verifying_key.to_montgomery().to_bytes())
}

fn ed25519_seed_to_x25519(seed: &[u8; 32]) -> [u8; 32] {
    // RFC 7748 / libsodium crypto_sign_ed25519_sk_to_curve25519:
    // 1. SHA-512(seed)
    // 2. Take lower 32 bytes
    // 3. Clamp: clear bits 0,1,2 of first byte; clear bit 7, set bit 6 of last byte
    let hash = Sha512::digest(seed);
    let mut x25519_sk = [0u8; 32];
    x25519_sk.copy_from_slice(&hash[..32]);
    x25519_sk[0] &= 248;
    x25519_sk[31] &= 127;
    x25519_sk[31] |= 64;
    x25519_sk
}

// ── New identity-based encrypt/decrypt ──────────────────────────────

/// Encrypt data for an identity using its Ed25519 public key.
///
/// Internally converts Ed25519 → X25519, then performs:
/// ECDH (ephemeral X25519) → HKDF-SHA256 (with random salt) → AES-256-GCM
///
/// The ephemeral public key is returned in X25519 SPKI format.
#[tauri::command]
pub fn encrypt_for_identity(
    plaintext_b64: String,
    identity_public_key_b64: String, // Ed25519 SPKI Base64
) -> Result<IdentitySealedData, String> {
    let plaintext = BASE64
        .decode(&plaintext_b64)
        .map_err(|e| format!("Invalid plaintext base64: {e}"))?;
    let spki_bytes = BASE64
        .decode(&identity_public_key_b64)
        .map_err(|e| format!("Invalid public key base64: {e}"))?;

    // Parse Ed25519 SPKI → raw bytes → convert to X25519
    let ed25519_raw = extract_ed25519_public_key_from_spki(&spki_bytes)?;
    let x25519_pk_bytes = ed25519_public_to_x25519(&ed25519_raw)?;
    let recipient_pk = PublicKey::from(x25519_pk_bytes);

    // Generate ephemeral X25519 keypair
    let mut eph_bytes = [0u8; 32];
    rand::fill(&mut eph_bytes);
    let ephemeral_secret = StaticSecret::from(eph_bytes);
    let ephemeral_public = PublicKey::from(&ephemeral_secret);

    // ECDH shared secret
    let shared_secret = ephemeral_secret.diffie_hellman(&recipient_pk);

    // Random salt
    let mut salt = [0u8; SALT_LENGTH];
    rand::fill(&mut salt);

    // HKDF-SHA256 with salt
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared_secret.as_bytes());
    let mut aes_key = [0u8; 32];
    hk.expand(HKDF_INFO, &mut aes_key)
        .map_err(|e| format!("HKDF expand failed: {e}"))?;

    // AES-256-GCM
    let cipher =
        Aes256Gcm::new_from_slice(&aes_key).map_err(|e| format!("AES init failed: {e}"))?;
    let mut iv = [0u8; IV_LENGTH];
    rand::fill(&mut iv);
    let nonce = Nonce::from_slice(&iv);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| format!("Encryption failed: {e}"))?;

    // Wrap ephemeral public key in X25519 SPKI
    let ephemeral_spki = wrap_x25519_public_key_as_spki(ephemeral_public.as_bytes());

    Ok(IdentitySealedData {
        encrypted_data: BASE64.encode(&ciphertext),
        nonce: BASE64.encode(&iv),
        salt: BASE64.encode(&salt),
        ephemeral_public_key: BASE64.encode(&ephemeral_spki),
    })
}

/// Decrypt data sealed for an identity using its Ed25519 private key.
///
/// Internally converts Ed25519 → X25519, then reverses the ECDH + HKDF + AES-GCM.
#[tauri::command]
pub fn decrypt_for_identity(
    encrypted_data_b64: String,
    nonce_b64: String,
    salt_b64: String,
    ephemeral_public_key_b64: String, // X25519 SPKI Base64
    identity_private_key_b64: String, // Ed25519 PKCS8 Base64
) -> Result<String, String> {
    let ciphertext = BASE64
        .decode(&encrypted_data_b64)
        .map_err(|e| format!("Invalid ciphertext base64: {e}"))?;
    let iv_bytes = BASE64
        .decode(&nonce_b64)
        .map_err(|e| format!("Invalid nonce base64: {e}"))?;
    let salt = BASE64
        .decode(&salt_b64)
        .map_err(|e| format!("Invalid salt base64: {e}"))?;
    let eph_spki = BASE64
        .decode(&ephemeral_public_key_b64)
        .map_err(|e| format!("Invalid ephemeral key base64: {e}"))?;
    let pkcs8_bytes = BASE64
        .decode(&identity_private_key_b64)
        .map_err(|e| format!("Invalid private key base64: {e}"))?;

    // Parse ephemeral X25519 SPKI → raw bytes
    let eph_raw = extract_x25519_public_key_from_spki(&eph_spki)?;
    let ephemeral_pk = PublicKey::from(eph_raw);

    // Convert Ed25519 PKCS8 seed → X25519 private key
    let seed = extract_ed25519_seed_from_pkcs8(&pkcs8_bytes)?;
    let x25519_sk_bytes = ed25519_seed_to_x25519(&seed);
    let private_key = StaticSecret::from(x25519_sk_bytes);

    // ECDH shared secret
    let shared_secret = private_key.diffie_hellman(&ephemeral_pk);

    // HKDF-SHA256 with salt
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared_secret.as_bytes());
    let mut aes_key = [0u8; 32];
    hk.expand(HKDF_INFO, &mut aes_key)
        .map_err(|e| format!("HKDF expand failed: {e}"))?;

    // AES-256-GCM decrypt
    let cipher =
        Aes256Gcm::new_from_slice(&aes_key).map_err(|e| format!("AES init failed: {e}"))?;
    if iv_bytes.len() != IV_LENGTH {
        return Err(format!(
            "Invalid nonce length: expected {IV_LENGTH}, got {}",
            iv_bytes.len()
        ));
    }
    let nonce = Nonce::from_slice(&iv_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| format!("Decryption failed: {e}"))?;

    Ok(BASE64.encode(&plaintext))
}

// ── Types ───────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentitySealedData {
    pub encrypted_data: String,
    pub nonce: String,
    pub salt: String,
    pub ephemeral_public_key: String,
}

#[cfg(test)]
mod tests;

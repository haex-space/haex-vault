use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

const IV_LENGTH: usize = 12;

/// Generate an X25519 keypair for key agreement.
/// Returns { publicKey, privateKey } as Base64-encoded raw bytes.
///
/// Used because WebCrypto X25519 is not yet supported in all WebViews
/// (notably webkit2gtk on Linux). Ed25519 signing stays in WebCrypto.
#[tauri::command]
pub fn generate_x25519_keypair() -> Result<X25519KeyPair, String> {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public_key = PublicKey::from(&secret);

    Ok(X25519KeyPair {
        public_key: BASE64.encode(public_key.as_bytes()),
        private_key: BASE64.encode(secret.as_bytes()),
    })
}

/// Encrypt data using X25519 ECDH + AES-256-GCM.
/// Generates an ephemeral keypair, derives a shared secret with the recipient's
/// public key, and encrypts the plaintext.
#[tauri::command]
pub fn x25519_encrypt(plaintext_b64: String, recipient_public_key_b64: String) -> Result<X25519Encrypted, String> {
    let plaintext = BASE64.decode(&plaintext_b64).map_err(|e| format!("Invalid plaintext base64: {e}"))?;
    let recipient_pk_bytes = BASE64.decode(&recipient_public_key_b64).map_err(|e| format!("Invalid public key base64: {e}"))?;

    if recipient_pk_bytes.len() != 32 {
        return Err(format!("Invalid public key length: expected 32, got {}", recipient_pk_bytes.len()));
    }

    let mut pk_array = [0u8; 32];
    pk_array.copy_from_slice(&recipient_pk_bytes);
    let recipient_pk = PublicKey::from(pk_array);

    // Generate ephemeral keypair
    let ephemeral_secret = StaticSecret::random_from_rng(OsRng);
    let ephemeral_public = PublicKey::from(&ephemeral_secret);

    // Derive shared secret via ECDH
    let shared_secret = ephemeral_secret.diffie_hellman(&recipient_pk);

    // Derive AES key from shared secret using HKDF
    let hk = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
    let mut aes_key = [0u8; 32];
    hk.expand(b"haex-vault-name-encryption", &mut aes_key)
        .map_err(|e| format!("HKDF expand failed: {e}"))?;

    // Encrypt with AES-256-GCM
    let cipher = Aes256Gcm::new_from_slice(&aes_key)
        .map_err(|e| format!("AES init failed: {e}"))?;

    let mut iv = [0u8; IV_LENGTH];
    rand::Fill::try_fill(&mut iv, &mut OsRng).map_err(|e| format!("RNG failed: {e}"))?;
    let nonce = Nonce::from_slice(&iv);

    let ciphertext = cipher.encrypt(nonce, plaintext.as_ref())
        .map_err(|e| format!("Encryption failed: {e}"))?;

    Ok(X25519Encrypted {
        encrypted_data: BASE64.encode(&ciphertext),
        nonce: BASE64.encode(&iv),
        ephemeral_public_key: BASE64.encode(ephemeral_public.as_bytes()),
    })
}

/// Decrypt data using X25519 ECDH + AES-256-GCM.
/// Uses the recipient's private key and the sender's ephemeral public key
/// to derive the shared secret and decrypt.
#[tauri::command]
pub fn x25519_decrypt(
    encrypted_data_b64: String,
    nonce_b64: String,
    ephemeral_public_key_b64: String,
    private_key_b64: String,
) -> Result<String, String> {
    let ciphertext = BASE64.decode(&encrypted_data_b64).map_err(|e| format!("Invalid ciphertext base64: {e}"))?;
    let iv_bytes = BASE64.decode(&nonce_b64).map_err(|e| format!("Invalid nonce base64: {e}"))?;
    let ephemeral_pk_bytes = BASE64.decode(&ephemeral_public_key_b64).map_err(|e| format!("Invalid ephemeral key base64: {e}"))?;
    let sk_bytes = BASE64.decode(&private_key_b64).map_err(|e| format!("Invalid private key base64: {e}"))?;

    if ephemeral_pk_bytes.len() != 32 || sk_bytes.len() != 32 || iv_bytes.len() != IV_LENGTH {
        return Err("Invalid key or nonce length".to_string());
    }

    let mut pk_array = [0u8; 32];
    pk_array.copy_from_slice(&ephemeral_pk_bytes);
    let ephemeral_pk = PublicKey::from(pk_array);

    let mut sk_array = [0u8; 32];
    sk_array.copy_from_slice(&sk_bytes);
    let private_key = StaticSecret::from(sk_array);

    // Derive shared secret
    let shared_secret = private_key.diffie_hellman(&ephemeral_pk);

    // Derive AES key using same HKDF parameters
    let hk = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
    let mut aes_key = [0u8; 32];
    hk.expand(b"haex-vault-name-encryption", &mut aes_key)
        .map_err(|e| format!("HKDF expand failed: {e}"))?;

    // Decrypt
    let cipher = Aes256Gcm::new_from_slice(&aes_key)
        .map_err(|e| format!("AES init failed: {e}"))?;

    let nonce = Nonce::from_slice(&iv_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| format!("Decryption failed: {e}"))?;

    Ok(BASE64.encode(&plaintext))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct X25519KeyPair {
    pub public_key: String,
    pub private_key: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct X25519Encrypted {
    pub encrypted_data: String,
    pub nonce: String,
    pub ephemeral_public_key: String,
}

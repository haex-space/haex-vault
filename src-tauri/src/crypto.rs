use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct X25519KeyPair {
    pub public_key: String,
    pub private_key: String,
}

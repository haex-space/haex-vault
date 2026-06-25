//! Base64 codec helpers shared across the leader submodules.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

pub(super) fn base64_encode(data: &[u8]) -> String {
    BASE64.encode(data)
}

pub(super) fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    BASE64
        .decode(s)
        .map_err(|e| format!("base64 decode error: {e}"))
}

//! Wire-format types for the QUIC DID-auth handshake.
//!
//! Two messages per handshake, length-prefixed JSON on a single bi-stream:
//!
//! 1. Server → Client (`Challenge`): protocol version, 32-byte random nonce,
//!    server endpoint id.
//! 2. Client → Server (`Response`): protocol version, client DID, client
//!    endpoint id, ed25519 signature over `build_sig_input(...)`.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const NONCE_LEN: usize = 32;

/// Domain-separation prefix. Prevents a signature accepted here from being
/// reusable as a UCAN, MLS Welcome, or any other ed25519-signed haex payload.
pub const DOMAIN_TAG: &[u8] = b"haex-did-auth/v1";

/// Cap on serialised handshake messages — handshake JSON is well under 1 KB,
/// 64 KB leaves slack but caps malicious senders.
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub v: u32,
    pub nonce: String,
    pub server_endpoint_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub v: u32,
    pub did: String,
    pub client_endpoint_id: String,
    pub signature: String,
}

/// Build the signature input for the handshake.
///
/// Layout: `DOMAIN_TAG || 0x00 || nonce || 0x00 || client_endpoint_id || 0x00 || server_endpoint_id`.
///
/// The `0x00` separators close the length-extension hole between the three
/// variable-length string fields — without them, an attacker who could
/// influence two adjacent fields could shift bytes across the boundary and
/// keep the signature valid.
pub fn build_sig_input(
    nonce: &[u8],
    client_endpoint_id: &str,
    server_endpoint_id: &str,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(
        DOMAIN_TAG.len() + 1 + nonce.len() + 1 + client_endpoint_id.len() + 1 + server_endpoint_id.len(),
    );
    buf.extend_from_slice(DOMAIN_TAG);
    buf.push(0x00);
    buf.extend_from_slice(nonce);
    buf.push(0x00);
    buf.extend_from_slice(client_endpoint_id.as_bytes());
    buf.push(0x00);
    buf.extend_from_slice(server_endpoint_id.as_bytes());
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sig_input_is_deterministic() {
        let a = build_sig_input(&[1, 2, 3], "client", "server");
        let b = build_sig_input(&[1, 2, 3], "client", "server");
        assert_eq!(a, b);
    }

    #[test]
    fn sig_input_separator_prevents_field_collision() {
        // "ab" || "" and "a" || "b" would collide without separators.
        let a = build_sig_input(&[0xAA], "ab", "cd");
        let b = build_sig_input(&[0xAA], "a", "bcd");
        assert_ne!(a, b);
    }

    #[test]
    fn sig_input_includes_domain_tag() {
        let input = build_sig_input(&[0], "c", "s");
        assert!(input.starts_with(DOMAIN_TAG));
    }

    #[test]
    fn challenge_roundtrip_json() {
        let c = Challenge {
            v: 1,
            nonce: "AAAA".into(),
            server_endpoint_id: "endpoint-srv".into(),
        };
        let json = serde_json::to_vec(&c).unwrap();
        let back: Challenge = serde_json::from_slice(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn response_roundtrip_json() {
        let r = Response {
            v: 1,
            did: "did:key:z6Mk".into(),
            client_endpoint_id: "endpoint-cli".into(),
            signature: "SIG".into(),
        };
        let json = serde_json::to_vec(&r).unwrap();
        let back: Response = serde_json::from_slice(&json).unwrap();
        assert_eq!(r, back);
    }
}

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
/// Layout: `DOMAIN_TAG || len(nonce) || nonce || len(client_ep) || client_ep || len(server_ep) || server_ep`,
/// where every `len` is a big-endian `u32`.
///
/// Length-prefixing every variable-length field is the only encoding that
/// guarantees collision-freeness regardless of what bytes the fields contain.
/// A previous version used `0x00` separators, which collides when any field
/// can carry an embedded NUL: `("a\0b", "c")` and `("a", "b\0c")` serialise
/// to the same bytes once concatenated through the separator scheme.
/// iroh endpoint ids are hex today so NUL bytes never appear in practice,
/// but the client controls the `client_endpoint_id` string carried in the
/// Response — defense in depth means we don't rely on that invariant.
pub fn build_sig_input(
    nonce: &[u8],
    client_endpoint_id: &str,
    server_endpoint_id: &str,
) -> Vec<u8> {
    let client_bytes = client_endpoint_id.as_bytes();
    let server_bytes = server_endpoint_id.as_bytes();
    let mut buf = Vec::with_capacity(
        DOMAIN_TAG.len() + 4 + nonce.len() + 4 + client_bytes.len() + 4 + server_bytes.len(),
    );
    buf.extend_from_slice(DOMAIN_TAG);
    buf.extend_from_slice(&(nonce.len() as u32).to_be_bytes());
    buf.extend_from_slice(nonce);
    buf.extend_from_slice(&(client_bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(client_bytes);
    buf.extend_from_slice(&(server_bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(server_bytes);
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
    fn sig_input_length_prefix_prevents_field_boundary_collision() {
        // Two splits of the same concatenation must produce different
        // sig-inputs under length-prefixing.
        let a = build_sig_input(&[0xAA], "ab", "cd");
        let b = build_sig_input(&[0xAA], "a", "bcd");
        assert_ne!(a, b);
    }

    #[test]
    fn sig_input_length_prefix_prevents_nul_byte_collision() {
        // Critical for defense in depth: embedded NUL in one string field
        // must not let an attacker shift bytes across the field boundary
        // and reuse a signature. Under the old 0x00-separator encoding,
        // ("a\0b", "c") and ("a", "b\0c") collided; length-prefixing
        // makes them distinct.
        let a = build_sig_input(&[0xAA], "a\0b", "c");
        let b = build_sig_input(&[0xAA], "a", "b\0c");
        assert_ne!(a, b);
    }

    #[test]
    fn sig_input_lengths_match_concrete_layout() {
        // Lock in the exact wire layout so a future "just refactor the
        // builder" change cannot silently break compatibility.
        let got = build_sig_input(&[0x11, 0x22], "ab", "cde");
        let mut expected = Vec::new();
        expected.extend_from_slice(DOMAIN_TAG);
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.extend_from_slice(&[0x11, 0x22]);
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.extend_from_slice(b"ab");
        expected.extend_from_slice(&3u32.to_be_bytes());
        expected.extend_from_slice(b"cde");
        assert_eq!(got, expected);
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

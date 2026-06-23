//! Shared helpers for the peer_storage_fullstack integration tests:
//! identity setup, UCAN minting, protocol comms, and the
//! `setup_server_client` fixture.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::LazyLock;
use tokio::time::Duration;

use ed25519_dalek::SigningKey;
use haex_vault_lib::peer_storage::endpoint::{OwnIdentity, PeerEndpoint};
use haex_vault_lib::peer_storage::protocol::{self, Request, Response, ALPN};
use haex_vault_lib::quic_did_auth;
use iroh::Endpoint;

pub(super) const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

/// Identity shared between `test_client_identity` and `test_ucan_token_for`
/// so the UCAN's `aud` and the client's verified DID line up under Layer
/// 1.25. Pulling the seed from the RNG once at first access keeps every
/// helper in this file in sync without any literal seed bytes — CodeQL
/// flagged the old `seed[0] = 42` pattern as a hardcoded credential.
pub(super) static SHARED_TEST_KEY: LazyLock<SigningKey> = LazyLock::new(|| {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
});

/// Build an OwnIdentity backed by `SHARED_TEST_KEY`.
pub(super) fn test_client_identity() -> OwnIdentity {
    let signing_key = SHARED_TEST_KEY.clone();
    let mut bytes = Vec::with_capacity(34);
    bytes.extend_from_slice(&ED25519_MULTICODEC);
    bytes.extend_from_slice(signing_key.verifying_key().as_bytes());
    let did = format!("did:key:z{}", bs58::encode(bytes).into_string());
    OwnIdentity { did, signing_key }
}

/// A fresh random identity for the server. The server's own DID does not
/// affect UCAN audience checks (those run against the client's DID), so a
/// new key per test isolates servers across the suite.
pub(super) fn random_server_identity() -> OwnIdentity {
    let seed: [u8; 32] = rand::random();
    let signing_key = SigningKey::from_bytes(&seed);
    let mut bytes = Vec::with_capacity(34);
    bytes.extend_from_slice(&ED25519_MULTICODEC);
    bytes.extend_from_slice(signing_key.verifying_key().as_bytes());
    let did = format!("did:key:z{}", bs58::encode(bytes).into_string());
    OwnIdentity { did, signing_key }
}

/// Install identities on the server and a slice of clients before starting
/// any of them, then start them in order. Returns the (shared) DID the
/// clients now claim. Callers wire that DID into `peer_owner_dids` for
/// every accepted client endpoint id so the Layer 1.5 cross-check passes.
pub(super) async fn install_test_identities(
    server: &mut PeerEndpoint,
    clients: &mut [&mut PeerEndpoint],
) -> String {
    server.set_own_identity(random_server_identity());
    let client_identity = test_client_identity();
    for c in clients.iter_mut() {
        c.set_own_identity(client_identity.clone());
    }
    server.start(None).await.unwrap();
    for c in clients.iter_mut() {
        c.start(None).await.unwrap();
    }
    client_identity.did
}

/// Variant for the multi-client tests that need to run the client-side
/// DID-auth handshake themselves (e.g. via raw protocol fixtures). Returns
/// the OwnIdentity instead of just the DID.
#[allow(dead_code)]
pub(super) async fn install_test_identities_full(
    server: &mut PeerEndpoint,
    clients: &mut [&mut PeerEndpoint],
) -> OwnIdentity {
    server.set_own_identity(random_server_identity());
    let client_identity = test_client_identity();
    for c in clients.iter_mut() {
        c.set_own_identity(client_identity.clone());
    }
    server.start(None).await.unwrap();
    for c in clients.iter_mut() {
        c.start(None).await.unwrap();
    }
    client_identity
}

/// Test UCAN token generator — creates a valid signed token for one or more spaces.
pub(super) fn test_ucan_token(space_id: &str) -> String {
    test_ucan_token_for(&[space_id])
}

/// Multi-space variant of [`test_ucan_token`]. The peer-storage handler now
/// gates each request on the intersection of the UCAN's claimed spaces and
/// the peer's allowed_spaces, so tests that operate on multiple spaces in
/// a single connection must present a UCAN that covers them all.
pub(super) fn test_ucan_token_for(space_ids: &[&str]) -> String {
    use base64::Engine;
    use ed25519_dalek::Signer;

    const BASE64URL: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        base64::engine::general_purpose::NO_PAD,
    );

    let vk = SHARED_TEST_KEY.verifying_key();
    let multicodec: [u8; 2] = [0xed, 0x01];
    let mut key_bytes = Vec::with_capacity(34);
    key_bytes.extend_from_slice(&multicodec);
    key_bytes.extend_from_slice(vk.as_bytes());
    let did = format!("did:key:z{}", bs58::encode(&key_bytes).into_string());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let cap: serde_json::Map<String, serde_json::Value> = space_ids
        .iter()
        .map(|s| {
            (
                format!("space:{}", s),
                serde_json::Value::String("space/admin".into()),
            )
        })
        .collect();
    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let payload = serde_json::json!({
        "ucv": "1.0",
        "iss": did,
        "aud": did,
        "cap": cap,
        "exp": now + 86400,
        "iat": now,
        "prf": [],
        "nnc": "test"
    });

    let h = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());
    let p = BASE64URL.encode(serde_json::to_string(&payload).unwrap().as_bytes());
    let sig = SHARED_TEST_KEY.sign(format!("{h}.{p}").as_bytes());
    format!("{h}.{p}.{}", BASE64URL.encode(sig.to_bytes()))
}

// =============================================================================
// Helper: proper protocol client
// =============================================================================

/// Connect to `server_addr` and run the quic_did_auth handshake on the
/// server-initiated bidirectional stream. Returns the QUIC connection ready
/// for application-level request streams.
pub(super) async fn connect_and_handshake(
    client_ep: &Endpoint,
    server_addr: iroh::EndpointAddr,
) -> Result<iroh::endpoint::Connection, String> {
    let conn = tokio::time::timeout(Duration::from_secs(5), client_ep.connect(server_addr, ALPN))
        .await
        .map_err(|_| "connect timeout".to_string())?
        .map_err(|e| format!("connect error: {e}"))?;

    // Server-initiated DID-auth stream: client accepts and signs.
    let (mut auth_send, mut auth_recv) =
        tokio::time::timeout(Duration::from_secs(5), conn.accept_bi())
            .await
            .map_err(|_| "auth accept_bi timeout".to_string())?
            .map_err(|e| format!("auth accept_bi: {e}"))?;

    let identity = test_client_identity();
    quic_did_auth::respond_to_challenge(
        &mut auth_send,
        &mut auth_recv,
        &identity.did,
        &identity.signing_key,
        &client_ep.id().to_string(),
    )
    .await
    .map_err(|e| format!("did-auth: {e}"))?;
    let _ = auth_send.finish();

    Ok(conn)
}

/// Send a protocol request and read the response using the correct wire format.
pub(super) async fn send_request(
    client_ep: &Endpoint,
    server_addr: iroh::EndpointAddr,
    request: &Request,
) -> Result<Response, String> {
    let conn = connect_and_handshake(client_ep, server_addr).await?;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open_bi error: {e}"))?;

    // Send request with length prefix
    let req_bytes = protocol::encode_request(request).map_err(|e| format!("encode: {e}"))?;
    send.write_all(&req_bytes)
        .await
        .map_err(|e| format!("write: {e}"))?;
    send.finish().map_err(|e| format!("finish: {e}"))?;

    // Read response with length prefix
    protocol::read_response(&mut recv)
        .await
        .map_err(|e| format!("read response: {e}"))
}

/// Send a READ request and return both the header and the file data bytes.
pub(super) async fn send_read_request(
    client_ep: &Endpoint,
    server_addr: iroh::EndpointAddr,
    path: &str,
    range: Option<[u64; 2]>,
) -> Result<(Response, Vec<u8>), String> {
    send_read_request_for_space(client_ep, server_addr, path, range, "space-1").await
}

/// Like `send_read_request`, but with a custom space ID for the UCAN token.
pub(super) async fn send_read_request_for_space(
    client_ep: &Endpoint,
    server_addr: iroh::EndpointAddr,
    path: &str,
    range: Option<[u64; 2]>,
    space_id: &str,
) -> Result<(Response, Vec<u8>), String> {
    let conn = connect_and_handshake(client_ep, server_addr).await?;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open_bi error: {e}"))?;

    let request = Request::Read {
        path: path.to_string(),
        range,
        ucan_token: test_ucan_token(space_id),
    };
    let req_bytes = protocol::encode_request(&request).map_err(|e| format!("encode: {e}"))?;
    send.write_all(&req_bytes)
        .await
        .map_err(|e| format!("write: {e}"))?;
    send.finish().map_err(|e| format!("finish: {e}"))?;

    // Read header
    let header: Response = protocol::read_response(&mut recv)
        .await
        .map_err(|e| format!("read header: {e}"))?;

    // Read file data
    let data = recv
        .read_to_end(10 * 1024 * 1024) // 10 MB max for tests
        .await
        .map_err(|e| format!("read data: {e}"))?;

    Ok((header, data))
}

/// Set up a server with a temp dir containing test files, allow a client, return everything.
pub(super) async fn setup_server_client(
    files: &[(&str, &[u8])],
    dirs: &[&str],
    share_name: &str,
    space_id: &str,
) -> (
    PeerEndpoint,
    PeerEndpoint,
    iroh::EndpointAddr,
    tempfile::TempDir,
) {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client = PeerEndpoint::new_ephemeral();

    let client_did = install_test_identities(&mut server, &mut [&mut client]).await;

    let tmp = tempfile::TempDir::new().unwrap();

    // Create directories
    for dir in dirs {
        std::fs::create_dir_all(tmp.path().join(dir)).unwrap();
    }

    // Create files
    for (path, content) in files {
        if let Some(parent) = PathBuf::from(path).parent() {
            std::fs::create_dir_all(tmp.path().join(parent)).ok();
        }
        std::fs::write(tmp.path().join(path), content).unwrap();
    }

    server
        .add_share(
            "share-1".to_string(),
            share_name.to_string(),
            tmp.path().to_string_lossy().to_string(),
            space_id.to_string(),
        )
        .await;

    // Allow client + matching DID expectation for the Layer 1.5 cross-check.
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert(space_id.to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(client.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let server_addr = server.endpoint_ref().unwrap().addr();

    (server, client, server_addr, tmp)
}

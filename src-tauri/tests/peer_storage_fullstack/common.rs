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
use haex_vault_lib::space_delivery::local::dos_defence::config::DosDefenceConfig;
use haex_vault_lib::ucan::space_id::{derive_space_id, NONCE_LEN};
use iroh::Endpoint;
use sha2::{Digest, Sha256};

/// Phase 2 DoS-defence caps would otherwise trip these tests in
/// unrelated ways — L1 per-source cap (10 conn/sec) flags any rapid
/// reconnect loop, L2 stream cap (8 in-flight) flags concurrent test
/// fixtures, etc. Layer enforcement has its own unit tests in
/// `peer_storage::endpoint::lifecycle::phase2_tests` and dedicated
/// fullstack tests in `dos_defence.rs`. Everywhere else we loosen the
/// caps so the test is exercising the thing it claims to.
pub(super) fn loose_dos_config() -> DosDefenceConfig {
    DosDefenceConfig {
        l1_global_rate_per_sec: u32::MAX,
        l1_per_source_rate_per_sec: u32::MAX,
        l2_max_streams_per_conn: u32::MAX,
        l3_handshake_timeout: Duration::from_secs(60),
        ..DosDefenceConfig::defaults()
    }
}

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

/// Space-Root key used to anchor every self-certifying `space_id` in this
/// suite. Independent of `SHARED_TEST_KEY` (the client's identity) so the
/// two-hop UCAN chain is a real delegation (root -> client) rather than a
/// self-issued token. Phase 2 `walk_prf_chain` requires the leaf's `iss`
/// to appear as a self-signed root somewhere up the `prf` chain and the
/// `space_id` to bind back to that root's DID.
pub(super) static SHARED_ROOT_KEY: LazyLock<SigningKey> = LazyLock::new(|| {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
});

/// DID of `SHARED_ROOT_KEY`. Every self-certifying `space_id` derived via
/// [`test_space_id`] embeds `sha256_16(domain || nonce || SHARED_ROOT_DID)`.
pub(super) static SHARED_ROOT_DID: LazyLock<String> = LazyLock::new(|| {
    let vk = SHARED_ROOT_KEY.verifying_key();
    let mut bytes = Vec::with_capacity(34);
    bytes.extend_from_slice(&ED25519_MULTICODEC);
    bytes.extend_from_slice(vk.as_bytes());
    format!("did:key:z{}", bs58::encode(bytes).into_string())
});

/// Map a stable, human-readable label (e.g. `"space-1"`, `"tier-basic"`)
/// to a self-certifying `space_id` anchored at `SHARED_ROOT_DID`. Callers
/// that reference the "same" logical space must use the same label so
/// `add_share`, `allowed_peers`, and the UCAN's `cap` all agree.
///
/// The nonce is derived deterministically from the label via `sha256(label)`
/// truncated to `NONCE_LEN` so the same label produces the same `space_id`
/// across every call within the test process.
///
/// The nonce is materialised directly from the hash slice via `try_into`
/// (never a zero-init buffer that's later overwritten) so CodeQL doesn't
/// flag the local as a hardcoded cryptographic value.
pub(super) fn test_space_id(label: &str) -> String {
    let hash = Sha256::digest(label.as_bytes());
    let nonce: [u8; NONCE_LEN] = hash[..NONCE_LEN]
        .try_into()
        .expect("sha256 output is 32 bytes, NONCE_LEN <= 32");
    derive_space_id(&SHARED_ROOT_DID, &nonce)
}

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
    server.set_dos_config(loose_dos_config()).await;
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
    server.set_dos_config(loose_dos_config()).await;
    for c in clients.iter_mut() {
        c.start(None).await.unwrap();
    }
    client_identity
}

/// Test UCAN token generator — creates a valid two-hop chain for a single
/// space label. See [`test_ucan_token_for`] for the multi-space variant.
pub(super) fn test_ucan_token(label: &str) -> String {
    test_ucan_token_for(&[label])
}

/// Multi-space variant of [`test_ucan_token`].
///
/// Builds the two-hop UCAN chain required by Phase 2 `walk_prf_chain`:
///
/// - Root: `iss = aud = SHARED_ROOT_DID`, `cap = space/admin` for each
///   `test_space_id(label)`, `prf = []` — the self-signed Space-Root the
///   walker terminates on.
/// - Leaf: `iss = SHARED_ROOT_DID`, `aud = SHARED_TEST_KEY.did` (the
///   client), same capabilities, `prf = [root_token]` — the delegated grant
///   the server actually receives.
///
/// Both tokens are signed by `SHARED_ROOT_KEY` and every `space_id` is
/// derived from `SHARED_ROOT_DID`, so `verify_space_id_binding` accepts the
/// terminal root.
///
/// Callers pass logical labels (e.g. `"space-1"`); [`test_space_id`] maps
/// them to their self-certifying encodings and the same mapping is used by
/// [`setup_server_client`] and [`send_read_request_for_space`].
pub(super) fn test_ucan_token_for(labels: &[&str]) -> String {
    use base64::Engine;
    use ed25519_dalek::Signer;

    const BASE64URL: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        base64::engine::general_purpose::NO_PAD,
    );

    let root_did: &str = &SHARED_ROOT_DID;
    let client_did = test_client_identity().did;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // `cap` covers every requested label; the same map is reused in the
    // root grant and the leaf delegation so the leaf never exceeds the
    // root's rights (a hard requirement of the Phase 2 walker).
    let cap: serde_json::Map<String, serde_json::Value> = labels
        .iter()
        .map(|label| {
            (
                format!("space:{}", test_space_id(label)),
                serde_json::Value::String("space/admin".into()),
            )
        })
        .collect();

    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let h = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());

    // Root: self-signed by SHARED_ROOT_KEY, iss == aud == SHARED_ROOT_DID.
    let root_payload = serde_json::json!({
        "ucv": "1.0",
        "iss": root_did,
        "aud": root_did,
        "cap": cap,
        "exp": now + 86400,
        "iat": now,
        "prf": [],
        "nnc": "test-root"
    });
    let p_root = BASE64URL.encode(serde_json::to_string(&root_payload).unwrap().as_bytes());
    let sig_root = SHARED_ROOT_KEY.sign(format!("{h}.{p_root}").as_bytes());
    let root_token = format!("{h}.{p_root}.{}", BASE64URL.encode(sig_root.to_bytes()));

    // Leaf: signed by SHARED_ROOT_KEY, delegated to the shared client DID.
    let leaf_payload = serde_json::json!({
        "ucv": "1.0",
        "iss": root_did,
        "aud": client_did,
        "cap": cap,
        "exp": now + 86400,
        "iat": now,
        "prf": [root_token],
        "nnc": "test-leaf"
    });
    let p_leaf = BASE64URL.encode(serde_json::to_string(&leaf_payload).unwrap().as_bytes());
    let sig_leaf = SHARED_ROOT_KEY.sign(format!("{h}.{p_leaf}").as_bytes());
    format!("{h}.{p_leaf}.{}", BASE64URL.encode(sig_leaf.to_bytes()))
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

/// Like `send_read_request`, but with an explicit space label for the UCAN
/// token. `label` is mapped through [`test_space_id`] so it matches what
/// [`setup_server_client`] registered for the same label.
pub(super) async fn send_read_request_for_space(
    client_ep: &Endpoint,
    server_addr: iroh::EndpointAddr,
    path: &str,
    range: Option<[u64; 2]>,
    label: &str,
) -> Result<(Response, Vec<u8>), String> {
    let conn = connect_and_handshake(client_ep, server_addr).await?;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open_bi error: {e}"))?;

    let request = Request::Read {
        path: path.to_string(),
        range,
        ucan_token: test_ucan_token(label),
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
///
/// `space_label` is a human-readable label; the server registers the share
/// under `test_space_id(space_label)` and grants the client access to the
/// same derived id. Callers pass the identical label to
/// [`test_ucan_token`] / [`send_read_request_for_space`] so the UCAN's
/// `cap` claim matches the server's expectation.
pub(super) async fn setup_server_client(
    files: &[(&str, &[u8])],
    dirs: &[&str],
    share_name: &str,
    space_label: &str,
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

    let space_id = test_space_id(space_label);

    server
        .add_share(
            "share-1".to_string(),
            share_name.to_string(),
            tmp.path().to_string_lossy().to_string(),
            space_id.clone(),
        )
        .await;

    // Allow client + matching DID expectation for the Layer 1.5 cross-check.
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert(space_id);
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(client.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let server_addr = server.endpoint_ref().unwrap().addr();

    (server, client, server_addr, tmp)
}

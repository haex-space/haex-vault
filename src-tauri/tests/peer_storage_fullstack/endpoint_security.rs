//! Endpoint security: leaked ID, unauthorized access, privilege escalation.

use std::collections::{HashMap, HashSet};
use tokio::time::{sleep, Duration};

use super::common::*;
use haex_vault_lib::peer_storage::endpoint::PeerEndpoint;
use haex_vault_lib::peer_storage::protocol::{Request, Response};

/// An attacker who knows the server's EndpointAddr but is NOT in allowed_peers
/// must be completely unable to list or read any files.
#[tokio::test]
async fn leaked_endpoint_id_cannot_access_files() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut legitimate = PeerEndpoint::new_ephemeral();
    let mut attacker = PeerEndpoint::new_ephemeral();

    let client_did =
        install_test_identities(&mut server, &mut [&mut legitimate, &mut attacker]).await;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("confidential.txt"), b"TOP SECRET DATA").unwrap();
    std::fs::write(tmp.path().join("passwords.db"), b"admin:hunter2").unwrap();

    server
        .add_share(
            "s1".to_string(),
            "Secrets".to_string(),
            tmp.path().to_string_lossy().to_string(),
            "private-space".to_string(),
        )
        .await;

    // Only legitimate peer is allowed; the attacker has its own iroh
    // endpoint but is not registered in allowed_peers, so the connection
    // is closed at the accept gate before the handshake even starts.
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("private-space".to_string());
    allowed.insert(legitimate.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(legitimate.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let server_addr = server.endpoint_ref().unwrap().addr();
    let attacker_ep = attacker.endpoint_ref().unwrap().clone();

    // Attempt LIST root
    let list_result = send_request(
        &attacker_ep,
        server_addr.clone(),
        &Request::List {
            path: "/".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await;
    match list_result {
        Err(_) => { /* Connection rejected */ }
        Ok(Response::List { entries }) => {
            assert!(
                entries.is_empty(),
                "Attacker must NOT see any shares, got: {:?}",
                entries
            );
        }
        Ok(Response::Error { .. }) => { /* Also acceptable */ }
        other => panic!("Attacker should be rejected, got: {:?}", other),
    }

    // Attempt to LIST inside a share by guessing the name
    let direct_result = send_request(
        &attacker_ep,
        server_addr.clone(),
        &Request::List {
            path: "/Secrets".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await;
    match direct_result {
        Err(_) | Ok(Response::Error { .. }) => { /* rejected */ }
        Ok(Response::List { entries }) => {
            assert!(entries.is_empty(), "Attacker must NOT list share contents");
        }
        other => panic!("Direct share access should fail, got: {:?}", other),
    }

    // Attempt to READ a specific file by guessing the path
    let read_result = send_read_request(
        &attacker_ep,
        server_addr.clone(),
        "/Secrets/confidential.txt",
        None,
    )
    .await;
    match read_result {
        Err(_) | Ok((Response::Error { .. }, _)) => { /* rejected */ }
        Ok((Response::ReadHeader { .. }, data)) => {
            assert!(data.is_empty(), "Attacker must NOT read file data!");
            assert_ne!(
                data, b"TOP SECRET DATA",
                "CRITICAL: attacker read confidential data!"
            );
        }
        other => panic!("File read should fail for attacker, got: {:?}", other),
    }

    // Attempt to STAT a file
    let stat_result = send_request(
        &attacker_ep,
        server_addr.clone(),
        &Request::Stat {
            path: "/Secrets/passwords.db".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await;
    match stat_result {
        Err(_) | Ok(Response::Error { .. }) => { /* rejected */ }
        Ok(Response::Stat { entry, .. }) => {
            panic!("CRITICAL: attacker got file metadata: {:?}", entry);
        }
        other => panic!("Stat should fail for attacker, got: {:?}", other),
    }

    // Legitimate peer CAN access
    let legit_ep = legitimate.endpoint_ref().unwrap().clone();
    let (_, data) = send_read_request_for_space(
        &legit_ep,
        server_addr,
        "/Secrets/confidential.txt",
        None,
        "private-space",
    )
    .await
    .unwrap();
    assert_eq!(data, b"TOP SECRET DATA");

    server.stop().await.ok();
}

/// Peer allowed in Space A must NOT access Space B shares,
/// even by guessing share names, IDs, or file paths.
#[tokio::test]
async fn space_a_peer_cannot_access_space_b_by_any_means() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut peer = PeerEndpoint::new_ephemeral();

    let client_did = install_test_identities(&mut server, &mut [&mut peer]).await;

    let tmp_pub = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp_pub.path().join("readme.txt"), b"public").unwrap();
    let tmp_priv = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp_priv.path().join("api_keys.env"), b"SECRET=abc123").unwrap();

    server
        .add_share(
            "pub".to_string(),
            "PublicDocs".to_string(),
            tmp_pub.path().to_string_lossy().to_string(),
            "space-public".to_string(),
        )
        .await;
    server
        .add_share(
            "priv".to_string(),
            "InternalOps".to_string(),
            tmp_priv.path().to_string_lossy().to_string(),
            "space-internal".to_string(),
        )
        .await;

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-public".to_string());
    allowed.insert(peer.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(peer.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let ep = peer.endpoint_ref().unwrap().clone();
    let addr = server.endpoint_ref().unwrap().addr();

    // Can access public
    let (_, data) = send_read_request_for_space(
        &ep,
        addr.clone(),
        "/PublicDocs/readme.txt",
        None,
        "space-public",
    )
    .await
    .unwrap();
    assert_eq!(data, b"public");

    // Cannot access internal — by share name.
    // Use a UCAN for "space-public" (the peer's REAL granted space) so the
    // rejection here is unambiguously the share->space cross-isolation check,
    // not a "UCAN is for an unknown space" failure.
    let resp = send_request(
        &ep,
        addr.clone(),
        &Request::List {
            path: "/InternalOps".to_string(),
            ucan_token: test_ucan_token("space-public"),
        },
    )
    .await
    .unwrap();
    match resp {
        Response::Error { .. } => { /* blocked */ }
        Response::List { entries } => panic!("LEAKED: listed internal files: {:?}", entries),
        other => panic!("Expected Error, got: {:?}", other),
    }

    // Cannot access internal — by share ID
    let resp = send_request(
        &ep,
        addr.clone(),
        &Request::List {
            path: "/priv".to_string(),
            ucan_token: test_ucan_token("space-public"),
        },
    )
    .await
    .unwrap();
    match resp {
        Response::Error { .. } => { /* blocked */ }
        Response::List { entries } => panic!("LEAKED via ID: {:?}", entries),
        other => panic!("Expected Error, got: {:?}", other),
    }

    // Cannot READ internal file directly
    let read = send_read_request(&ep, addr.clone(), "/InternalOps/api_keys.env", None).await;
    match read {
        Err(_) | Ok((Response::Error { .. }, _)) => { /* blocked */ }
        Ok((_, data)) => {
            assert_ne!(
                String::from_utf8_lossy(&data),
                "SECRET=abc123",
                "CRITICAL: read internal file from wrong space!"
            );
        }
    }

    server.stop().await.ok();
}

/// After access revocation, repeated attempts on an existing connection all fail.
#[tokio::test]
async fn revoked_peer_stays_blocked_across_multiple_attempts() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client = PeerEndpoint::new_ephemeral();

    let client_did = install_test_identities(&mut server, &mut [&mut client]).await;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("secret.txt"), b"pre-revoke").unwrap();

    server
        .add_share(
            "s1".to_string(),
            "Data".to_string(),
            tmp.path().to_string_lossy().to_string(),
            "space-1".to_string(),
        )
        .await;

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(client.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let ep = client.endpoint_ref().unwrap().clone();
    let addr = server.endpoint_ref().unwrap().addr();

    // Works before revoke
    let (_, data) = send_read_request(&ep, addr.clone(), "/Data/secret.txt", None)
        .await
        .unwrap();
    assert_eq!(data, b"pre-revoke");

    // Revoke
    server.set_allowed_peers(HashMap::new()).await;
    server.set_peer_owner_dids(HashMap::new()).await;
    sleep(Duration::from_millis(100)).await;

    // Update file to detect stale data
    std::fs::write(tmp.path().join("secret.txt"), b"post-revoke").unwrap();

    // 5 attempts — all must fail
    for i in 0..5 {
        let list = send_request(
            &ep,
            addr.clone(),
            &Request::List {
                path: "/".to_string(),
                ucan_token: test_ucan_token("space-1"),
            },
        )
        .await;
        match list {
            Err(_) => { /* rejected */ }
            Ok(Response::Error { .. }) => { /* rejected */ }
            Ok(Response::List { entries }) => {
                assert!(
                    entries.is_empty(),
                    "Attempt {i}: revoked peer sees shares: {:?}",
                    entries
                );
            }
            other => panic!("Attempt {i}: unexpected: {:?}", other),
        }

        let read = send_read_request(&ep, addr.clone(), "/Data/secret.txt", None).await;
        match read {
            Err(_) | Ok((Response::Error { .. }, _)) => { /* blocked */ }
            Ok((_, data)) => {
                assert!(data.is_empty(), "Attempt {i}: revoked peer read data!");
            }
        }
    }

    server.stop().await.ok();
}

/// 3 peers in 3 spaces: complete mutual isolation verified.
#[tokio::test]
async fn three_peers_three_spaces_complete_isolation() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut pa = PeerEndpoint::new_ephemeral();
    let mut pb = PeerEndpoint::new_ephemeral();
    let mut pc = PeerEndpoint::new_ephemeral();

    let client_did = install_test_identities(&mut server, &mut [&mut pa, &mut pb, &mut pc]).await;

    let ta = tempfile::TempDir::new().unwrap();
    std::fs::write(ta.path().join("a.txt"), b"alice").unwrap();
    let tb = tempfile::TempDir::new().unwrap();
    std::fs::write(tb.path().join("b.txt"), b"bob").unwrap();
    let tc = tempfile::TempDir::new().unwrap();
    std::fs::write(tc.path().join("c.txt"), b"charlie").unwrap();

    server
        .add_share(
            "sa".to_string(),
            "Alice".to_string(),
            ta.path().to_string_lossy().to_string(),
            "sp-a".to_string(),
        )
        .await;
    server
        .add_share(
            "sb".to_string(),
            "Bob".to_string(),
            tb.path().to_string_lossy().to_string(),
            "sp-b".to_string(),
        )
        .await;
    server
        .add_share(
            "sc".to_string(),
            "Charlie".to_string(),
            tc.path().to_string_lossy().to_string(),
            "sp-c".to_string(),
        )
        .await;

    let mut allowed = HashMap::new();
    let mut sa = HashSet::new();
    sa.insert("sp-a".to_string());
    let mut sb = HashSet::new();
    sb.insert("sp-b".to_string());
    let mut sc = HashSet::new();
    sc.insert("sp-c".to_string());
    allowed.insert(pa.endpoint_id().to_string(), sa);
    allowed.insert(pb.endpoint_id().to_string(), sb);
    allowed.insert(pc.endpoint_id().to_string(), sc);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(pa.endpoint_id().to_string(), client_did.clone());
    owner_dids.insert(pb.endpoint_id().to_string(), client_did.clone());
    owner_dids.insert(pc.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let addr = server.endpoint_ref().unwrap().addr();
    let ea = pa.endpoint_ref().unwrap().clone();
    let eb = pb.endpoint_ref().unwrap().clone();
    let ec = pc.endpoint_ref().unwrap().clone();

    // Each sees only their share. UCAN claims the peer's authorized space
    // — the per-UCAN-space gate rejects any UCAN that doesn't intersect
    // with allowed_spaces.
    for (label, ep, expected, file, content, space) in [
        ("Alice", &ea, "Alice", "a.txt", b"alice".as_slice(), "sp-a"),
        ("Bob", &eb, "Bob", "b.txt", b"bob".as_slice(), "sp-b"),
        (
            "Charlie",
            &ec,
            "Charlie",
            "c.txt",
            b"charlie".as_slice(),
            "sp-c",
        ),
    ] {
        let resp = send_request(
            ep,
            addr.clone(),
            &Request::List {
                path: "/".to_string(),
                ucan_token: test_ucan_token(space),
            },
        )
        .await
        .unwrap();
        match &resp {
            Response::List { entries } => {
                assert_eq!(entries.len(), 1, "{label} should see exactly 1 share");
                assert_eq!(entries[0].name, expected);
            }
            other => panic!("{label}: Expected List, got: {:?}", other),
        }
        let (_, data) = send_read_request_for_space(
            ep,
            addr.clone(),
            &format!("/{expected}/{file}"),
            None,
            space,
        )
        .await
        .unwrap();
        assert_eq!(data, content, "{label}: wrong file content");
    }

    // Cross-access attempts: Alice→Bob, Bob→Charlie, Charlie→Alice
    for (label, ep, target_share, target_file, forbidden_content) in [
        ("Alice→Bob", &ea, "Bob", "b.txt", b"bob".as_slice()),
        (
            "Bob→Charlie",
            &eb,
            "Charlie",
            "c.txt",
            b"charlie".as_slice(),
        ),
        ("Charlie→Alice", &ec, "Alice", "a.txt", b"alice".as_slice()),
    ] {
        let read = send_read_request(
            ep,
            addr.clone(),
            &format!("/{target_share}/{target_file}"),
            None,
        )
        .await;
        match read {
            Err(_) | Ok((Response::Error { .. }, _)) => { /* blocked — correct */ }
            Ok((_, data)) => {
                assert_ne!(
                    data, forbidden_content,
                    "CRITICAL: {label} cross-space data leak!"
                );
            }
        }
    }

    server.stop().await.ok();
}

/// Dynamic access: peer gets upgraded from Space A to Space A+B, then downgraded back.
#[tokio::test]
async fn dynamic_space_grant_upgrade_and_downgrade() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut user = PeerEndpoint::new_ephemeral();

    let client_did = install_test_identities(&mut server, &mut [&mut user]).await;

    let tmp_a = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp_a.path().join("basic.txt"), b"basic").unwrap();
    let tmp_b = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp_b.path().join("premium.txt"), b"premium").unwrap();

    server
        .add_share(
            "sa".to_string(),
            "Basic".to_string(),
            tmp_a.path().to_string_lossy().to_string(),
            "tier-basic".to_string(),
        )
        .await;
    server
        .add_share(
            "sb".to_string(),
            "Premium".to_string(),
            tmp_b.path().to_string_lossy().to_string(),
            "tier-premium".to_string(),
        )
        .await;

    let addr = server.endpoint_ref().unwrap().addr();
    let ep = user.endpoint_ref().unwrap().clone();

    // Phase 1: Basic only
    let mut allowed = HashMap::new();
    let mut basic_only = HashSet::new();
    basic_only.insert("tier-basic".to_string());
    allowed.insert(user.endpoint_id().to_string(), basic_only);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(user.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let resp = send_request(
        &ep,
        addr.clone(),
        &Request::List {
            path: "/".to_string(),
            ucan_token: test_ucan_token("tier-basic"),
        },
    )
    .await
    .unwrap();
    match &resp {
        Response::List { entries } => assert_eq!(entries.len(), 1, "Phase 1: only basic"),
        other => panic!("Phase 1: {:?}", other),
    }

    // Phase 2: Upgrade to basic + premium
    let mut upgraded = HashMap::new();
    let mut both = HashSet::new();
    both.insert("tier-basic".to_string());
    both.insert("tier-premium".to_string());
    upgraded.insert(user.endpoint_id().to_string(), both);
    server.set_allowed_peers(upgraded).await;
    sleep(Duration::from_millis(50)).await;

    let resp = send_request(
        &ep,
        addr.clone(),
        &Request::List {
            path: "/".to_string(),
            // Phase 2 presents a UCAN covering BOTH tiers — the new
            // per-UCAN-space gate restricts the root listing to the
            // intersection of allowed_spaces and the UCAN's claimed
            // spaces, so a single-tier UCAN here would only return the
            // matching share.
            ucan_token: test_ucan_token_for(&["tier-basic", "tier-premium"]),
        },
    )
    .await
    .unwrap();
    match &resp {
        Response::List { entries } => assert_eq!(entries.len(), 2, "Phase 2: basic + premium"),
        other => panic!("Phase 2: {:?}", other),
    }
    let (_, data) = send_read_request_for_space(
        &ep,
        addr.clone(),
        "/Premium/premium.txt",
        None,
        "tier-premium",
    )
    .await
    .unwrap();
    assert_eq!(data, b"premium");

    // Phase 3: Downgrade back to basic only
    let mut downgraded = HashMap::new();
    let mut basic_only = HashSet::new();
    basic_only.insert("tier-basic".to_string());
    downgraded.insert(user.endpoint_id().to_string(), basic_only);
    server.set_allowed_peers(downgraded).await;
    sleep(Duration::from_millis(50)).await;

    let resp = send_request(
        &ep,
        addr.clone(),
        &Request::List {
            path: "/".to_string(),
            ucan_token: test_ucan_token("tier-basic"),
        },
    )
    .await
    .unwrap();
    match &resp {
        Response::List { entries } => {
            assert_eq!(entries.len(), 1, "Phase 3: back to basic only");
            assert_eq!(entries[0].name, "Basic");
        }
        other => panic!("Phase 3: {:?}", other),
    }

    // Premium should be inaccessible again
    let premium_read = send_read_request(&ep, addr, "/Premium/premium.txt", None).await;
    match premium_read {
        Err(_) | Ok((Response::Error { .. }, _)) => { /* blocked — correct */ }
        Ok((_, data)) => {
            assert_ne!(
                data, b"premium",
                "CRITICAL: downgraded user still reads premium!"
            );
        }
    }

    server.stop().await.ok();
}

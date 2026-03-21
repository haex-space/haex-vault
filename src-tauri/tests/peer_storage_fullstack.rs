//! Full-stack P2P Storage integration tests.
//!
//! Tests the complete protocol: LIST, STAT, READ with proper length-prefixed encoding.
//! Covers: nested directories, file downloads, chunked transfers, path traversal prevention,
//! cross-space isolation, multi-share browsing, concurrent connections.
//!
//! Run: cargo test --test peer_storage_fullstack

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

use iroh::Endpoint;
use haex_vault_lib::peer_storage::endpoint::PeerEndpoint;
use haex_vault_lib::peer_storage::protocol::{self, FileEntry, Request, Response, ALPN};

// =============================================================================
// Helper: proper protocol client
// =============================================================================

/// Send a protocol request and read the response using the correct wire format.
async fn send_request(
    client_ep: &Endpoint,
    server_addr: iroh::EndpointAddr,
    request: &Request,
) -> Result<Response, String> {
    let conn = tokio::time::timeout(
        Duration::from_secs(5),
        client_ep.connect(server_addr, ALPN),
    )
    .await
    .map_err(|_| "connect timeout".to_string())?
    .map_err(|e| format!("connect error: {e}"))?;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open_bi error: {e}"))?;

    // Send request with length prefix
    let req_bytes = protocol::encode_request(request)
        .map_err(|e| format!("encode: {e}"))?;
    send.write_all(&req_bytes)
        .await
        .map_err(|e| format!("write: {e}"))?;
    send.finish()
        .map_err(|e| format!("finish: {e}"))?;

    // Read response with length prefix
    protocol::read_response(&mut recv)
        .await
        .map_err(|e| format!("read response: {e}"))
}

/// Send a READ request and return both the header and the file data bytes.
async fn send_read_request(
    client_ep: &Endpoint,
    server_addr: iroh::EndpointAddr,
    path: &str,
    range: Option<[u64; 2]>,
) -> Result<(Response, Vec<u8>), String> {
    let conn = tokio::time::timeout(
        Duration::from_secs(5),
        client_ep.connect(server_addr, ALPN),
    )
    .await
    .map_err(|_| "connect timeout".to_string())?
    .map_err(|e| format!("connect error: {e}"))?;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open_bi error: {e}"))?;

    let request = Request::Read { path: path.to_string(), range };
    let req_bytes = protocol::encode_request(&request)
        .map_err(|e| format!("encode: {e}"))?;
    send.write_all(&req_bytes)
        .await
        .map_err(|e| format!("write: {e}"))?;
    send.finish()
        .map_err(|e| format!("finish: {e}"))?;

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
async fn setup_server_client(
    files: &[(&str, &[u8])],
    dirs: &[&str],
    share_name: &str,
    space_id: &str,
) -> (PeerEndpoint, PeerEndpoint, iroh::EndpointAddr, tempfile::TempDir) {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client = PeerEndpoint::new_ephemeral();

    server.start(None).await.unwrap();
    client.start(None).await.unwrap();

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

    server.add_share(
        "share-1".to_string(),
        share_name.to_string(),
        tmp.path().to_path_buf(),
        space_id.to_string(),
    ).await;

    // Allow client
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert(space_id.to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    let server_addr = server.endpoint_ref().unwrap().addr();

    (server, client, server_addr, tmp)
}

// =============================================================================
// LIST Tests
// =============================================================================

#[tokio::test]
async fn list_root_shows_shared_folders() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[("readme.txt", b"hello")],
        &[],
        "Documents",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(&client_ep, addr, &Request::List { path: "/".to_string() }).await.unwrap();

    match resp {
        Response::List { entries } => {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "Documents");
            assert!(entries[0].is_dir);
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn list_share_shows_files_and_dirs() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[
            ("file1.txt", b"content1"),
            ("file2.md", b"# Header"),
            ("subdir/nested.txt", b"nested"),
        ],
        &["emptydir"],
        "MyShare",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(&client_ep, addr, &Request::List { path: "/MyShare".to_string() }).await.unwrap();

    match resp {
        Response::List { entries } => {
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"file1.txt"), "missing file1.txt, got: {:?}", names);
            assert!(names.contains(&"file2.md"), "missing file2.md, got: {:?}", names);
            assert!(names.contains(&"subdir"), "missing subdir, got: {:?}", names);
            assert!(names.contains(&"emptydir"), "missing emptydir, got: {:?}", names);

            let file1 = entries.iter().find(|e| e.name == "file1.txt").unwrap();
            assert!(!file1.is_dir);
            assert_eq!(file1.size, 8); // "content1"

            let subdir = entries.iter().find(|e| e.name == "subdir").unwrap();
            assert!(subdir.is_dir);
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn list_nested_directory() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[
            ("deep/level1/level2/file.txt", b"deep file"),
            ("deep/level1/sibling.txt", b"sibling"),
        ],
        &[],
        "DeepShare",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    // List /DeepShare/deep/level1
    let resp = send_request(
        &client_ep, addr,
        &Request::List { path: "/DeepShare/deep/level1".to_string() },
    ).await.unwrap();

    match resp {
        Response::List { entries } => {
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"level2"), "missing level2 dir");
            assert!(names.contains(&"sibling.txt"), "missing sibling.txt");
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn list_nonexistent_path_returns_error() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[("file.txt", b"x")],
        &[],
        "Share",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep, addr,
        &Request::List { path: "/Share/nonexistent".to_string() },
    ).await.unwrap();

    match resp {
        Response::Error { message } => {
            assert!(message.contains("not found") || message.contains("Not a directory"),
                "Unexpected error: {}", message);
        }
        other => panic!("Expected Error, got: {:?}", other),
    }

    let _ = server.stop().await;
}

// =============================================================================
// STAT Tests
// =============================================================================

#[tokio::test]
async fn stat_file_returns_metadata() {
    let content = b"Hello, World! This is a test file.";
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[("hello.txt", content)],
        &[],
        "StatTest",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep, addr,
        &Request::Stat { path: "/StatTest/hello.txt".to_string() },
    ).await.unwrap();

    match resp {
        Response::Stat { entry } => {
            assert_eq!(entry.name, "hello.txt");
            assert!(!entry.is_dir);
            assert_eq!(entry.size, content.len() as u64);
            assert!(entry.modified.is_some());
        }
        other => panic!("Expected Stat, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn stat_directory_returns_metadata() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[("subdir/file.txt", b"x")],
        &[],
        "StatDir",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep, addr,
        &Request::Stat { path: "/StatDir/subdir".to_string() },
    ).await.unwrap();

    match resp {
        Response::Stat { entry } => {
            assert_eq!(entry.name, "subdir");
            assert!(entry.is_dir);
        }
        other => panic!("Expected Stat, got: {:?}", other),
    }

    let _ = server.stop().await;
}

// =============================================================================
// READ Tests
// =============================================================================

#[tokio::test]
async fn read_small_file() {
    let content = b"Hello, P2P World!";
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[("greeting.txt", content)],
        &[],
        "ReadTest",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, data) = send_read_request(
        &client_ep, addr, "/ReadTest/greeting.txt", None,
    ).await.unwrap();

    match header {
        Response::ReadHeader { size } => {
            assert_eq!(size, content.len() as u64);
        }
        other => panic!("Expected ReadHeader, got: {:?}", other),
    }

    assert_eq!(data, content);

    let _ = server.stop().await;
}

#[tokio::test]
async fn read_large_file_chunked() {
    // 256 KB file — will be sent in multiple 64 KB chunks
    let content: Vec<u8> = (0..256 * 1024).map(|i| (i % 256) as u8).collect();
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[("large.bin", &content)],
        &[],
        "LargeFile",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, data) = send_read_request(
        &client_ep, addr, "/LargeFile/large.bin", None,
    ).await.unwrap();

    match header {
        Response::ReadHeader { size } => {
            assert_eq!(size, content.len() as u64);
        }
        other => panic!("Expected ReadHeader, got: {:?}", other),
    }

    assert_eq!(data.len(), content.len());
    assert_eq!(data, content);

    let _ = server.stop().await;
}

#[tokio::test]
async fn read_with_byte_range() {
    let content = b"0123456789ABCDEF";
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[("range.txt", content)],
        &[],
        "RangeTest",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    // Read bytes 4..12 (8 bytes: "4567890A" — wait, "89AB")
    let (header, data) = send_read_request(
        &client_ep, addr, "/RangeTest/range.txt", Some([4, 12]),
    ).await.unwrap();

    match header {
        Response::ReadHeader { size } => {
            assert_eq!(size, 8); // 12 - 4
        }
        other => panic!("Expected ReadHeader, got: {:?}", other),
    }

    assert_eq!(data, b"456789AB");

    let _ = server.stop().await;
}

#[tokio::test]
async fn read_nonexistent_file_returns_error() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[("exists.txt", b"x")],
        &[],
        "ReadErr",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, _) = send_read_request(
        &client_ep, addr, "/ReadErr/missing.txt", None,
    ).await.unwrap();

    match header {
        Response::Error { message } => {
            assert!(message.contains("not found") || message.contains("Not a file"),
                "Unexpected error: {}", message);
        }
        other => panic!("Expected Error, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn read_directory_returns_error() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[("subdir/file.txt", b"x")],
        &[],
        "ReadDir",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, _) = send_read_request(
        &client_ep, addr, "/ReadDir/subdir", None,
    ).await.unwrap();

    match header {
        Response::Error { message } => {
            assert!(message.contains("Not a file"), "Unexpected error: {}", message);
        }
        other => panic!("Expected Error, got: {:?}", other),
    }

    let _ = server.stop().await;
}

// =============================================================================
// Security Tests
// =============================================================================

#[tokio::test]
async fn path_traversal_is_blocked() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[("safe.txt", b"safe")],
        &[],
        "Secure",
        "space-1",
    ).await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    // Try to escape the share with ../
    let resp = send_request(
        &client_ep, addr,
        &Request::List { path: "/Secure/../../../etc".to_string() },
    ).await.unwrap();

    match resp {
        Response::Error { message } => {
            assert!(
                message.contains("denied") || message.contains("outside") || message.contains("not found"),
                "Path traversal should be blocked, got: {}", message,
            );
        }
        other => panic!("Path traversal should return Error, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn cross_space_isolation() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client = PeerEndpoint::new_ephemeral();

    server.start(None).await.unwrap();
    client.start(None).await.unwrap();

    let tmp1 = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp1.path().join("public.txt"), b"public").unwrap();

    let tmp2 = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp2.path().join("secret.txt"), b"secret").unwrap();

    // Add two shares in different spaces
    server.add_share("s1".to_string(), "Public".to_string(), tmp1.path().to_path_buf(), "space-public".to_string()).await;
    server.add_share("s2".to_string(), "Private".to_string(), tmp2.path().to_path_buf(), "space-private".to_string()).await;

    // Client only has access to space-public
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-public".to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let server_addr = server.endpoint_ref().unwrap().addr();

    // List root — should only show Public
    let resp = send_request(&client_ep, server_addr.clone(), &Request::List { path: "/".to_string() }).await.unwrap();
    match resp {
        Response::List { entries } => {
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"Public"), "Should see Public share");
            assert!(!names.contains(&"Private"), "Should NOT see Private share, got: {:?}", names);
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    // Try to access Private directly — should fail
    let resp = send_request(&client_ep, server_addr, &Request::List { path: "/Private".to_string() }).await.unwrap();
    match resp {
        Response::Error { .. } => { /* expected */ }
        other => panic!("Accessing Private share should fail, got: {:?}", other),
    }

    let _ = server.stop().await;
}

// =============================================================================
// Multi-Share Tests
// =============================================================================

#[tokio::test]
async fn multiple_shares_in_same_space() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client = PeerEndpoint::new_ephemeral();

    server.start(None).await.unwrap();
    client.start(None).await.unwrap();

    let tmp1 = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp1.path().join("doc.txt"), b"document").unwrap();

    let tmp2 = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp2.path().join("photo.jpg"), b"\xFF\xD8\xFF\xE0").unwrap();

    server.add_share("s1".to_string(), "Documents".to_string(), tmp1.path().to_path_buf(), "shared-space".to_string()).await;
    server.add_share("s2".to_string(), "Photos".to_string(), tmp2.path().to_path_buf(), "shared-space".to_string()).await;

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("shared-space".to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let server_addr = server.endpoint_ref().unwrap().addr();

    // List root — should show both shares
    let resp = send_request(&client_ep, server_addr.clone(), &Request::List { path: "/".to_string() }).await.unwrap();
    match resp {
        Response::List { entries } => {
            assert_eq!(entries.len(), 2);
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"Documents"));
            assert!(names.contains(&"Photos"));
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    // Read file from each share
    let (_, doc_data) = send_read_request(&client_ep, server_addr.clone(), "/Documents/doc.txt", None).await.unwrap();
    assert_eq!(doc_data, b"document");

    let (_, photo_data) = send_read_request(&client_ep, server_addr, "/Photos/photo.jpg", None).await.unwrap();
    assert_eq!(photo_data, b"\xFF\xD8\xFF\xE0");

    let _ = server.stop().await;
}

// =============================================================================
// Concurrent Connection Tests
// =============================================================================

#[tokio::test]
async fn concurrent_clients_can_connect() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client1 = PeerEndpoint::new_ephemeral();
    let mut client2 = PeerEndpoint::new_ephemeral();

    server.start(None).await.unwrap();
    client1.start(None).await.unwrap();
    client2.start(None).await.unwrap();

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("shared.txt"), b"shared content").unwrap();

    server.add_share("s1".to_string(), "Shared".to_string(), tmp.path().to_path_buf(), "space-1".to_string()).await;

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert(client1.endpoint_id().to_string(), spaces.clone());
    allowed.insert(client2.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    let server_addr = server.endpoint_ref().unwrap().addr();
    let ep1 = client1.endpoint_ref().unwrap().clone();
    let ep2 = client2.endpoint_ref().unwrap().clone();

    // Both clients connect and read simultaneously
    let (r1, r2) = tokio::join!(
        send_read_request(&ep1, server_addr.clone(), "/Shared/shared.txt", None),
        send_read_request(&ep2, server_addr, "/Shared/shared.txt", None),
    );

    let (_, data1) = r1.unwrap();
    let (_, data2) = r2.unwrap();

    assert_eq!(data1, b"shared content");
    assert_eq!(data2, b"shared content");

    let _ = server.stop().await;
}

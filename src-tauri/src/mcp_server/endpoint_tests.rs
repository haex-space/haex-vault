use uuid::Uuid;

use super::endpoint::{endpoint_name, peer_allowed, ENDPOINT_PREFIX};

#[test]
fn the_name_is_short_and_tied_to_the_device() {
    let device = Uuid::parse_str("0123456789ab4def8123456789abcdef").expect("uuid");
    assert_eq!(endpoint_name(device), "mcp-0123456789ab");
    assert!(endpoint_name(Uuid::new_v4()).starts_with(ENDPOINT_PREFIX));
}

#[test]
fn only_the_owner_may_talk_to_the_endpoint() {
    assert!(peer_allowed(1000, 1000));
    assert!(!peer_allowed(1001, 1000));
    assert!(!peer_allowed(0, 1000), "not even root by uid");
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::mcp_server::endpoint::{bind, connect, discover, socket_path};

    #[tokio::test]
    async fn the_socket_lives_in_a_private_directory_and_is_found() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("holzi-mcp");
        let device = Uuid::new_v4();
        let mut listener = bind(&dir, device).await.expect("bind");
        let mode = std::fs::metadata(&dir).expect("dir").permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let path = socket_path(&dir, &endpoint_name(device));
        assert_eq!(listener.path(), path);
        assert!(
            path.as_os_str().len() <= 104 || !cfg!(target_os = "macos"),
            "macOS limits socket paths"
        );
        assert_eq!(discover(&dir), [endpoint_name(device)]);

        let client = tokio::spawn({
            let dir = dir.clone();
            async move { connect(&dir, &endpoint_name(device)).await }
        });
        let accepted = listener.accept().await.expect("accept");
        assert!(accepted.is_some(), "the same user is let in");
        client.await.expect("join").expect("connected");

        drop(listener);
        assert!(!path.exists(), "the socket goes with the listener");
    }

    #[tokio::test]
    async fn a_left_over_socket_is_replaced_and_a_live_one_is_kept() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("holzi-mcp");
        let device = Uuid::new_v4();
        let live = bind(&dir, device).await.expect("first");
        let error = bind(&dir, device).await.err().expect("a live socket stays");
        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
        drop(live);

        // A crash leaves the file behind with nobody listening: a std listener does not remove it.
        let path = socket_path(&dir, &endpoint_name(device));
        drop(std::os::unix::net::UnixListener::bind(&path).expect("stale socket"));
        assert!(path.exists());
        bind(&dir, device)
            .await
            .expect("the stale file is replaced");
    }

    #[test]
    fn the_endpoint_never_names_a_network_address() {
        let dir = std::path::Path::new("/run/user/1000/holzi-mcp");
        let path = socket_path(dir, "mcp-0123456789ab");
        let text = path.to_string_lossy();
        assert!(text.ends_with(".sock"));
        assert!(!text.contains(':'), "no host:port");
    }
}

#[cfg(windows)]
#[test]
fn the_pipe_name_is_local() {
    use super::endpoint::pipe_name;
    assert_eq!(
        pipe_name("mcp-0123456789ab"),
        r"\\.\pipe\holzi-mcp-0123456789ab"
    );
}

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use super::{discard_partial, download_to_file, part_path};

/// Reads a full request head. A single `read` can return a partial header
/// block, which would make the Range assertions below flaky.
fn read_request_head(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => head.push(byte[0]),
            Err(e) => panic!("read request head: {e}"),
        }
    }
    String::from_utf8_lossy(&head).into_owned()
}

#[tokio::test]
async fn resumes_after_a_truncated_response_body() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().expect("first request");
        let request_text = read_request_head(&mut first);
        assert!(
            !request_text.contains("range: bytes="),
            "first request must not ask for a range: {request_text}"
        );
        first
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n0123",
            )
            .expect("write truncated response");
        drop(first);

        let (mut second, _) = listener.accept().expect("resume request");
        let request_text = read_request_head(&mut second);
        assert!(
            request_text.contains("range: bytes=4-"),
            "resume must continue at byte 4: {request_text}"
        );
        second
            .write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Length: 6\r\nContent-Range: bytes 4-9/10\r\nConnection: close\r\n\r\n456789",
            )
            .expect("write resumed response");
    });

    // Bound to a local so the guard outlives the download; `tempdir()` deletes
    // its directory the moment the `TempDir` is dropped.
    let workspace = tempfile::tempdir().expect("tempdir");
    let destination = workspace.path().join("model.gguf");
    let result = download_to_file(
        &format!("http://{address}/model.gguf"),
        destination.clone(),
        |_| {},
    )
    .await
    .expect("download should resume");

    // Asserted before the join: a regression that stops requesting bytes
    // leaves the server thread blocked in `accept`, and joining first would
    // turn a plain assertion failure into a hung test.
    assert_eq!(result.bytes_written, 10);
    assert_eq!(
        std::fs::read(destination).expect("downloaded file"),
        b"0123456789"
    );
    server.join().expect("test server thread");
}

/// A server may answer a Range request with fewer bytes than were asked for
/// and end the body cleanly — nothing errors. Only comparing the transferred
/// size against the announced total keeps that short `.part` from being
/// promoted to a finalized, corrupt GGUF.
#[tokio::test]
async fn keeps_resuming_when_a_body_ends_cleanly_but_short() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().expect("first request");
        read_request_head(&mut first);
        first
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n0123",
            )
            .expect("write truncated response");
        drop(first);

        // Complete body, cleanly closed — but only two of the six remaining
        // bytes. `Content-Range` still announces the full ten.
        let (mut second, _) = listener.accept().expect("second request");
        let request_text = read_request_head(&mut second);
        assert!(
            request_text.contains("range: bytes=4-"),
            "second request must resume at byte 4: {request_text}"
        );
        second
            .write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Length: 2\r\nContent-Range: bytes 4-5/10\r\nConnection: close\r\n\r\n45",
            )
            .expect("write capped range response");
        drop(second);

        let (mut third, _) = listener.accept().expect("third request");
        let request_text = read_request_head(&mut third);
        assert!(
            request_text.contains("range: bytes=6-"),
            "a short body must be resumed, not accepted: {request_text}"
        );
        third
            .write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 6-9/10\r\nConnection: close\r\n\r\n6789",
            )
            .expect("write final range response");
    });

    let workspace = tempfile::tempdir().expect("tempdir");
    let destination = workspace.path().join("model.gguf");
    let result = download_to_file(
        &format!("http://{address}/model.gguf"),
        destination.clone(),
        |_| {},
    )
    .await
    .expect("download should keep resuming until the announced size is met");

    // See the note above: assert before joining so accepting a short body
    // fails the test instead of hanging it.
    assert_eq!(result.bytes_written, 10);
    assert_eq!(
        std::fs::read(&destination).expect("downloaded file"),
        b"0123456789"
    );
    assert!(
        !workspace.path().join("model.gguf.part").exists(),
        "the .part sidecar must not survive a finalized download"
    );
    server.join().expect("test server thread");
}

/// A 404 answers the same way on every retry. Burning the attempt budget on
/// it would only delay the error the operator needs to see — and the error
/// message proves it: a retry would have hit the closed listener and reported
/// a connection failure instead of the status.
#[tokio::test]
async fn does_not_retry_a_permanent_status() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("first request");
        read_request_head(&mut stream);
        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .expect("write not-found response");
    });

    let workspace = tempfile::tempdir().expect("tempdir");
    let destination = workspace.path().join("model.gguf");
    let error = download_to_file(
        &format!("http://{address}/model.gguf"),
        destination.clone(),
        |_| {},
    )
    .await
    .expect_err("a 404 must fail the download");

    server.join().expect("test server thread");
    assert!(
        error.to_string().contains("404"),
        "the error should name the status, not a retry's connect failure: {error}"
    );
    assert!(
        !destination.exists(),
        "a failed download must not finalize a file"
    );
}

#[tokio::test]
async fn restarts_from_zero_when_a_partial_response_has_no_validator() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().expect("first request");
        read_request_head(&mut first);
        first
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123")
            .expect("write unvalidated truncated response");
        drop(first);

        let (mut second, _) = listener.accept().expect("restart request");
        let request_text = read_request_head(&mut second);
        assert!(
            !request_text.contains("range: bytes="),
            "a partial response without a validator must restart from zero: {request_text}"
        );
        assert!(
            !request_text.contains("if-range:"),
            "a restart must not send If-Range: {request_text}"
        );
        second
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789",
            )
            .expect("write complete response");
    });

    let workspace = tempfile::tempdir().expect("tempdir");
    let destination = workspace.path().join("model.gguf");
    let result = download_to_file(
        &format!("http://{address}/model.gguf"),
        destination.clone(),
        |_| {},
    )
    .await
    .expect("download should restart without a validator");

    assert_eq!(result.bytes_written, 10);
    assert_eq!(
        std::fs::read(destination).expect("downloaded file"),
        b"0123456789"
    );
    server.join().expect("test server thread");
}

#[tokio::test]
async fn restarts_when_a_range_response_has_invalid_content_range() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().expect("first request");
        read_request_head(&mut first);
        first
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n0123",
            )
            .expect("write truncated response");
        drop(first);

        let (mut second, _) = listener.accept().expect("range request");
        let request_text = read_request_head(&mut second);
        assert!(
            request_text.contains("range: bytes=4-"),
            "the validated partial file should be resumed: {request_text}"
        );
        second
            .write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Length: 2\r\nConnection: close\r\n\r\n45",
            )
            .expect("write response without content range");
        drop(second);

        let (mut third, _) = listener.accept().expect("restart request");
        let request_text = read_request_head(&mut third);
        assert!(
            !request_text.contains("range: bytes="),
            "invalid range metadata must restart from zero: {request_text}"
        );
        third
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n0123456789",
            )
            .expect("write complete response");
    });

    let workspace = tempfile::tempdir().expect("tempdir");
    let destination = workspace.path().join("model.gguf");
    let result = download_to_file(
        &format!("http://{address}/model.gguf"),
        destination.clone(),
        |_| {},
    )
    .await
    .expect("download should reject invalid range metadata");

    assert_eq!(result.bytes_written, 10);
    assert_eq!(
        std::fs::read(destination).expect("downloaded file"),
        b"0123456789"
    );
    server.join().expect("test server thread");
}

/// Closing the vault drops a running transfer (`VaultGate::run`, spec 013 FR-008). Only the
/// `.part` sidecar may remain: nothing that looks like an installed model.
#[tokio::test]
async fn a_download_dropped_mid_transfer_leaves_no_finalized_file() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let (release, released) = std::sync::mpsc::channel::<()>();
    let server = thread::spawn(move || {
        let (mut request, _) = listener.accept().expect("request");
        read_request_head(&mut request);
        request
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n0123",
            )
            .expect("write the first bytes");
        // The body stays unfinished until the test is done.
        let _ = released.recv();
    });

    let workspace = tempfile::tempdir().expect("tempdir");
    let destination = workspace.path().join("model.gguf");
    let outcome = tokio::time::timeout(
        Duration::from_millis(300),
        download_to_file(
            &format!("http://{address}/model.gguf"),
            destination.clone(),
            |_| {},
        ),
    )
    .await;

    assert!(
        outcome.is_err(),
        "the transfer was still running when it was dropped"
    );
    assert!(
        !destination.exists(),
        "a dropped transfer must never leave a finalized file"
    );
    release.send(()).expect("release the server");
    server.join().expect("test server thread");
}

/// Cancelling stops the transfer by dropping it; the caller then discards the `.part` sidecar, so a
/// cancelled download leaves nothing behind.
#[tokio::test]
async fn discarding_a_cancelled_download_removes_its_partial_file() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let (release, released) = std::sync::mpsc::channel::<()>();
    let server = thread::spawn(move || {
        let (mut request, _) = listener.accept().expect("request");
        read_request_head(&mut request);
        request
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n0123",
            )
            .expect("write the first bytes");
        let _ = released.recv();
    });

    let workspace = tempfile::tempdir().expect("tempdir");
    let destination = workspace.path().join("model.gguf");
    let outcome = tokio::time::timeout(
        Duration::from_millis(300),
        download_to_file(
            &format!("http://{address}/model.gguf"),
            destination.clone(),
            |_| {},
        ),
    )
    .await;
    assert!(outcome.is_err(), "the transfer was cancelled mid-body");
    assert!(
        part_path(&destination).exists(),
        "a transfer that stopped midway leaves its sidecar"
    );

    discard_partial(&destination).await;

    assert!(!part_path(&destination).exists());
    // Idempotent: a second discard of what is already gone is no error.
    discard_partial(&destination).await;
    release.send(()).expect("release the server");
    server.join().expect("test server thread");
}

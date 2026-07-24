use super::helpers::*;

#[tokio::test]
async fn pipe_reader_to_send_cancelled_token_aborts_upload() {
    let h = setup_harness().await;
    let upload_path = format!("/{}/cancel_upload.bin", h.share_name);

    // 8 MB payload — large enough that the pipeline iterates over multiple
    // chunks (CHUNK_SIZE = 1 MB) so the cancel-check between chunks gets a
    // chance to trip. Pre-cancelling the token still works because the
    // check runs before the first chunk write.
    let payload: Vec<u8> = (0..(8 * 1024 * 1024)).map(|i| (i % 251) as u8).collect();
    let src_dir = tempfile::tempdir().unwrap();
    let src_path = src_dir.path().join("cancel_payload.bin");
    tokio::fs::write(&src_path, &payload).await.unwrap();

    // Reuse the harness's Space-Root signer + space_id so the write UCAN's
    // chain walk resolves to a root DID that binds to the same space the
    // server registered `share_name` under.
    let write_token = write_ucan(&h.ucan_root_signer, &h.space_id, &h.client_did);

    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();

    let options = crate::peer_storage::streaming::SendOptions {
        on_progress: None,
        cancel_token: Some(token),
    };

    let result = h
        .client
        .remote_write_file(
            h.server_remote_id,
            None,
            &upload_path,
            &src_path,
            &write_token,
            options,
        )
        .await;

    assert!(result.is_err(), "cancelled upload must return an error");
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("cancel") || err_str.contains("Cancel"),
        "error must mention cancellation, got: {err_str}"
    );

    // The server stages writes to a `.part` sibling and only renames on
    // success — a cancelled upload must leave neither the staged file nor
    // the final destination on disk. Server cleanup runs asynchronously
    // after the client's connection reset propagates, so poll briefly
    // before asserting absence.
    let dest = h._tmp.path().join("cancel_upload.bin");
    let staged = h._tmp.path().join("cancel_upload.bin.part");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if !dest.exists() && !staged.exists() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "cancelled upload left files on disk after 2s: dest_exists={}, staged_exists={}",
                dest.exists(),
                staged.exists()
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

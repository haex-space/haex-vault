use super::*;

#[tokio::test]
async fn sidecar_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("file.bin");

    let state = PartialState {
        file_hash: "abc123".into(),
        chunk_size: 1024 * 1024,
        completed: vec![true, false, true, false],
    };
    state.save(&target).await.unwrap();

    let loaded = PartialState::load(&target).await.unwrap();
    assert_eq!(loaded, Some(state));

    PartialState::clear(&target).await.unwrap();
    assert_eq!(PartialState::load(&target).await.unwrap(), None);
}

#[tokio::test]
async fn load_returns_none_for_corrupt_json() {
    // A sidecar that fails to parse (torn write from an older non-atomic
    // save(), bit-rot, or external edit) must be treated as missing so the
    // caller can start from a clean slate. Returning an error here would
    // permanently wedge resume for that file.
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("file.bin");
    let meta = tmp.path().join("file.bin.haex-partial.meta");
    tokio::fs::write(&meta, b"this is not json {{{").await.unwrap();

    let result = PartialState::load(&target).await.unwrap();
    assert!(result.is_none(), "corrupt sidecar must be treated as missing");
}

#[tokio::test]
async fn sidecar_load_returns_none_on_hash_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("file.bin");
    let state = PartialState {
        file_hash: "old".into(),
        chunk_size: 1,
        completed: vec![true],
    };
    state.save(&target).await.unwrap();

    let other_hash_state = PartialState::load_if_matches(&target, "new").await.unwrap();
    assert_eq!(
        other_hash_state, None,
        "mismatched hash → caller resumes from scratch"
    );
}

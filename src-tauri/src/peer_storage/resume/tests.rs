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

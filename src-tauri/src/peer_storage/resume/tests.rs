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
    tokio::fs::write(&meta, b"this is not json {{{")
        .await
        .unwrap();

    let result = PartialState::load(&target).await.unwrap();
    assert!(
        result.is_none(),
        "corrupt sidecar must be treated as missing"
    );
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

#[tokio::test]
async fn sweep_tmp_removes_orphans_keeps_pair_and_siblings() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("file.bin");

    // Orphaned atomic-write temp files left by interrupted save() calls, plus
    // the authoritative .meta + the partial byte stream, plus an unrelated
    // file's temp orphan that must NOT be touched.
    let meta = tmp.path().join("file.bin.haex-partial.meta");
    let partial = tmp.path().join("file.bin.haex-partial");
    let other = tmp.path().join("other.bin.haex-partial.meta.tmp.1");
    tokio::fs::write(&meta, b"{}").await.unwrap();
    tokio::fs::write(&partial, b"partial").await.unwrap();
    tokio::fs::write(&other, b"torn").await.unwrap();
    let orphans: Vec<_> = [46u64, 55, 189]
        .iter()
        .map(|n| {
            tmp.path()
                .join(format!("file.bin.haex-partial.meta.tmp.{n}"))
        })
        .collect();
    for o in &orphans {
        tokio::fs::write(o, b"torn").await.unwrap();
    }

    PartialState::sweep_tmp(&target).await.unwrap();

    for o in &orphans {
        assert!(!o.exists(), "orphaned temp file must be swept: {o:?}");
    }
    assert!(meta.exists(), "sweep_tmp must keep the authoritative .meta");
    assert!(partial.exists(), "sweep_tmp must keep the .haex-partial");
    assert!(
        other.exists(),
        "sweep_tmp must not touch another file's temp orphan"
    );
}

#[tokio::test]
async fn clear_also_sweeps_temp_files() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("file.bin");

    let state = PartialState {
        file_hash: "abc".into(),
        chunk_size: 1024,
        completed: vec![true, false],
    };
    state.save(&target).await.unwrap();
    let orphan = tmp.path().join("file.bin.haex-partial.meta.tmp.7");
    tokio::fs::write(&orphan, b"torn").await.unwrap();

    PartialState::clear(&target).await.unwrap();

    assert_eq!(PartialState::load(&target).await.unwrap(), None);
    assert!(
        !orphan.exists(),
        "clear() must sweep orphaned temp files too"
    );
}

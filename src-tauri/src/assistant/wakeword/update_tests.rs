use super::*;

fn part(content: &[u8]) -> Part {
    Part {
        bytes: content.len() as u64,
        sha256: hex(&Sha256::digest(content)),
    }
}

fn manifest(version: u32, embedding: &[u8], classifier: &[u8]) -> Manifest {
    Manifest {
        version,
        revision: "a".repeat(40),
        embedding: part(embedding),
        classifier: part(classifier),
    }
}

/// Puts a version's files where an update leaves them, without recording it.
fn stage(root: &Path, manifest: &Manifest, embedding: &[u8], classifier: &[u8]) {
    let dir = manifest.dir(root);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(EMBEDDING_FILE), embedding).unwrap();
    std::fs::write(dir.join(CLASSIFIER_FILE), classifier).unwrap();
}

#[test]
fn the_bundled_models_are_the_pinned_version() {
    let files = bundled();
    assert_eq!(files.version, BUNDLED_VERSION);
    assert_eq!(
        hex(&Sha256::digest(&files.embedding)),
        "092fa584098a0c0e5c5382d028eed7e7e7d5e41ccf23786e7417248d0aabe8c6"
    );
    assert_eq!(
        hex(&Sha256::digest(&files.classifier)),
        "c67e7a3686741c9ff2ea14de714611b54c64e7ddae3eb4522015e02c0e9ec70a"
    );
}

#[test]
fn a_manifest_needs_a_commit_and_well_formed_files() {
    let good = serde_json::to_string(&manifest(2, b"e", b"c")).unwrap();
    assert_eq!(Manifest::parse(&good).unwrap().version, 2);

    let branch = good.replace(&"a".repeat(40), "main");
    assert!(Manifest::parse(&branch).is_err(), "a branch moves");
    let no_hash = good.replace(&part(b"e").sha256, "nothex");
    assert!(Manifest::parse(&no_hash).is_err());
    let empty = serde_json::to_string(&Manifest {
        embedding: Part {
            bytes: 0,
            sha256: part(b"e").sha256,
        },
        ..manifest(2, b"e", b"c")
    })
    .unwrap();
    assert!(Manifest::parse(&empty).is_err());
    assert!(Manifest::parse("{}").is_err());
}

#[test]
fn without_an_update_the_bundled_models_run() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(select(Some(root.path())).version, BUNDLED_VERSION);
    assert_eq!(select(None).version, BUNDLED_VERSION);
}

#[test]
fn a_committed_newer_version_is_used() {
    let root = tempfile::tempdir().unwrap();
    let next = manifest(BUNDLED_VERSION + 1, b"embedding", b"classifier");
    stage(root.path(), &next, b"embedding", b"classifier");
    commit(root.path(), &next).unwrap();

    let files = select(Some(root.path()));
    assert_eq!(files.version, BUNDLED_VERSION + 1);
    assert_eq!(&*files.embedding, b"embedding");
    assert_eq!(&*files.classifier, b"classifier");
}

#[test]
fn a_version_that_is_staged_but_not_recorded_is_not_used() {
    let root = tempfile::tempdir().unwrap();
    let next = manifest(BUNDLED_VERSION + 1, b"embedding", b"classifier");
    stage(root.path(), &next, b"embedding", b"classifier");
    assert_eq!(select(Some(root.path())).version, BUNDLED_VERSION);
}

#[test]
fn a_damaged_file_is_refused_and_the_bundled_models_run() {
    let root = tempfile::tempdir().unwrap();
    let next = manifest(BUNDLED_VERSION + 1, b"embedding", b"classifier");
    stage(root.path(), &next, b"embedding", b"tampered!!!");
    assert!(commit(root.path(), &next).is_err());
    assert!(
        !next.dir(root.path()).exists(),
        "the damaged files are removed"
    );
    assert_eq!(select(Some(root.path())).version, BUNDLED_VERSION);

    // Damaged after it was recorded: the record no longer matches, so the bundled ones run.
    stage(root.path(), &next, b"embedding", b"classifier");
    commit(root.path(), &next).unwrap();
    std::fs::write(next.dir(root.path()).join(CLASSIFIER_FILE), b"tampered!!!").unwrap();
    assert_eq!(select(Some(root.path())).version, BUNDLED_VERSION);
}

#[test]
fn a_version_not_newer_than_the_binary_is_ignored() {
    // After an app release bundles a newer version than the one downloaded earlier.
    let root = tempfile::tempdir().unwrap();
    let old = manifest(BUNDLED_VERSION, b"embedding", b"classifier");
    stage(root.path(), &old, b"embedding", b"classifier");
    commit(root.path(), &old).unwrap();
    assert_eq!(&*select(Some(root.path())).embedding, BUNDLED_EMBEDDING);
}

#[test]
fn committing_removes_older_versions() {
    let root = tempfile::tempdir().unwrap();
    let first = manifest(BUNDLED_VERSION + 1, b"e1", b"c1");
    stage(root.path(), &first, b"e1", b"c1");
    commit(root.path(), &first).unwrap();
    let second = manifest(BUNDLED_VERSION + 2, b"e2", b"c2");
    stage(root.path(), &second, b"e2", b"c2");
    commit(root.path(), &second).unwrap();

    assert!(!first.dir(root.path()).exists());
    assert_eq!(select(Some(root.path())).version, BUNDLED_VERSION + 2);
}

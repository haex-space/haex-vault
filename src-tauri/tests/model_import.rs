//! Finalized model files must survive unsuccessful imports and self-imports.

#[path = "common/chosen_files.rs"]
mod chosen_files;

use chosen_files::{chosen, Paths};

use haex_lib::models::import::{cleanup_staging_in_dir, copy_into_managed};

#[tokio::test]
async fn reimporting_the_managed_file_preserves_its_contents() {
    let dir = tempfile::tempdir().expect("temp dir");
    let model = dir.path().join("model.gguf");
    let contents = b"GGUF existing model bytes";
    tokio::fs::write(&model, contents)
        .await
        .expect("seed model");

    let copied = copy_into_managed(Paths, chosen(&model), model.clone())
        .await
        .expect("reimport managed file");

    assert_eq!(copied, contents.len() as u64);
    assert_eq!(tokio::fs::read(&model).await.expect("read model"), contents);
}

#[tokio::test]
async fn successful_import_replaces_only_the_destination() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("source.gguf");
    let destination = dir.path().join("managed.gguf");
    let contents = b"GGUF replacement model";
    tokio::fs::write(&source, contents).await.expect("source");
    tokio::fs::write(&destination, b"old model")
        .await
        .expect("destination");

    assert_eq!(
        copy_into_managed(Paths, chosen(&source), destination.clone())
            .await
            .expect("import"),
        contents.len() as u64,
    );
    assert_eq!(
        tokio::fs::read(&destination)
            .await
            .expect("read destination"),
        contents
    );
    assert_eq!(
        tokio::fs::read(&source).await.expect("read source"),
        contents
    );
}

#[tokio::test]
async fn failed_publication_preserves_destination_and_removes_staging() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("source.gguf");
    let destination = dir.path().join("managed.gguf");
    tokio::fs::write(&source, b"GGUF replacement")
        .await
        .expect("source");
    tokio::fs::create_dir(&destination)
        .await
        .expect("destination directory");
    let existing = destination.join("keep");
    tokio::fs::write(&existing, b"existing")
        .await
        .expect("existing file");

    assert!(copy_into_managed(Paths, chosen(&source), destination)
        .await
        .is_err());
    assert_eq!(
        tokio::fs::read(existing).await.expect("read existing"),
        b"existing"
    );
    let mut entries = tokio::fs::read_dir(dir.path()).await.expect("read dir");
    while let Some(entry) = entries.next_entry().await.expect("entry") {
        assert_ne!(
            entry.path().extension().and_then(|e| e.to_str()),
            Some("tmp")
        );
    }
}

#[test]
fn startup_cleanup_removes_model_staging_files_only() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let models = tmp.path().join("models");
    let slug = models.join("model-slug");
    std::fs::create_dir_all(&slug).expect("model directory");
    std::fs::write(slug.join("model.gguf.tmp"), b"partial").expect("staging file");
    std::fs::write(slug.join("model.gguf"), b"published").expect("model file");
    std::fs::write(models.join("unrelated.tmp"), b"untouched").expect("unrelated file");

    assert_eq!(cleanup_staging_in_dir(&models).expect("cleanup"), 1);
    assert!(!slug.join("model.gguf.tmp").exists());
    assert!(slug.join("model.gguf").exists());
    assert!(models.join("unrelated.tmp").exists());
}

/// T065/T071: `.locks/` (the cross-process publication lock directory, spec 013 US4) is never
/// scanned for staging leftovers — a lock file that happened to end in `.tmp` must survive.
#[test]
fn startup_cleanup_never_touches_the_locks_directory() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let models = tmp.path().join("models");
    let locks = models.join(".locks");
    std::fs::create_dir_all(&locks).expect("locks directory");
    let lock_file = locks.join("model-slug.lock");
    std::fs::write(&lock_file, b"lock").expect("lock file");
    // Deliberately staging-shaped, to prove the directory itself is skipped rather than its files
    // happening not to match the staging suffixes.
    let tmp_shaped = locks.join("model-slug.lock.tmp");
    std::fs::write(&tmp_shaped, b"still a lock").expect("tmp-shaped lock file");

    assert_eq!(cleanup_staging_in_dir(&models).expect("cleanup"), 0);
    assert!(lock_file.exists());
    assert!(tmp_shaped.exists());
}

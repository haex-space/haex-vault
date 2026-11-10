use std::path::Path;

use super::{drafter_dir, drafter_file, path_for_load, remove, state, DrafterState};
use crate::catalog::CatalogDrafter;
use crate::models::hash::sha256_bytes_hex;
use crate::models::paths::canonical_model_file_in;

const SLUG: &str = "gemma-4-e2b-instruct-q4_k_m";
const BYTES: &[u8] = b"a drafter stands in here";

fn pin(bytes: &[u8]) -> CatalogDrafter {
    CatalogDrafter {
        hf_repo: "unsloth/gemma-4-E2B-it-GGUF".into(),
        hf_revision: "0314792d7f1f7e229411f620751375812bb9faf2".into(),
        hf_filename: "mtp-gemma-4-E2B-it.gguf".into(),
        size_bytes: bytes.len() as u64,
        sha256: sha256_bytes_hex(bytes),
        architecture: "gemma4-assistant".into(),
        measured_partly_on_gpu: false,
    }
}

fn put(root: &Path, drafter: &CatalogDrafter, bytes: &[u8]) {
    let file = drafter_file(root, SLUG, drafter).expect("path");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
    std::fs::write(file, bytes).expect("write");
}

#[test]
fn the_drafter_lives_in_its_models_own_folder() {
    let root = Path::new("/models");
    let drafter = pin(BYTES);
    assert_eq!(
        drafter_dir(root, SLUG).expect("dir"),
        root.join(SLUG).join("drafter")
    );
    assert_eq!(
        drafter_file(root, SLUG, &drafter).expect("file"),
        root.join(SLUG)
            .join("drafter")
            .join("mtp-gemma-4-E2B-it.gguf")
    );
}

#[test]
fn installed_means_the_file_has_exactly_the_pinned_size() {
    let root = tempfile::tempdir().expect("tempdir");
    let drafter = pin(BYTES);
    assert_eq!(
        state(root.path(), SLUG, &drafter).expect("state"),
        DrafterState::Missing
    );
    put(root.path(), &drafter, b"short");
    assert_eq!(
        state(root.path(), SLUG, &drafter).expect("state"),
        DrafterState::Missing
    );
    put(root.path(), &drafter, BYTES);
    assert_eq!(
        state(root.path(), SLUG, &drafter).expect("state"),
        DrafterState::Installed
    );
}

#[test]
fn a_drafter_with_the_pinned_hash_is_loaded() {
    let root = tempfile::tempdir().expect("tempdir");
    let drafter = pin(BYTES);
    put(root.path(), &drafter, BYTES);
    assert_eq!(
        path_for_load(root.path(), SLUG, &drafter),
        Some(drafter_file(root.path(), SLUG, &drafter).expect("file"))
    );
}

#[test]
fn a_damaged_drafter_is_not_loaded_and_its_folder_is_removed() {
    let root = tempfile::tempdir().expect("tempdir");
    let drafter = pin(BYTES);
    // Same size, other bytes: only the hash can tell.
    let damaged: Vec<u8> = BYTES.iter().map(|b| b ^ 1).collect();
    put(root.path(), &drafter, &damaged);
    assert_eq!(path_for_load(root.path(), SLUG, &drafter), None);
    assert!(!drafter_dir(root.path(), SLUG).expect("dir").exists());
}

#[test]
fn a_missing_or_short_drafter_is_not_loaded_and_its_folder_stays() {
    let root = tempfile::tempdir().expect("tempdir");
    let drafter = pin(BYTES);
    assert_eq!(path_for_load(root.path(), SLUG, &drafter), None);
    put(root.path(), &drafter, b"short");
    assert_eq!(path_for_load(root.path(), SLUG, &drafter), None);
    // A short file may be a download still being published; only a wrong hash removes the folder.
    assert!(drafter_file(root.path(), SLUG, &drafter)
        .expect("file")
        .exists());
}

#[test]
fn a_hostile_slug_or_file_name_is_refused() {
    let root = tempfile::tempdir().expect("tempdir");
    let mut drafter = pin(BYTES);
    assert!(drafter_dir(root.path(), "../x").is_err());
    assert!(state(root.path(), "../x", &drafter).is_err());
    assert_eq!(path_for_load(root.path(), "../x", &drafter), None);
    assert!(remove(root.path(), "../x").is_err());
    drafter.hf_filename = "../model.gguf".into();
    assert!(drafter_file(root.path(), SLUG, &drafter).is_err());
}

#[test]
fn removing_a_missing_drafter_succeeds() {
    let root = tempfile::tempdir().expect("tempdir");
    remove(root.path(), SLUG).expect("nothing to remove");
    let drafter = pin(BYTES);
    put(root.path(), &drafter, BYTES);
    remove(root.path(), SLUG).expect("remove");
    assert!(!drafter_dir(root.path(), SLUG).expect("dir").exists());
}

#[test]
fn the_model_file_is_still_found_next_to_a_drafter_folder() {
    let root = tempfile::tempdir().expect("tempdir");
    let drafter = pin(BYTES);
    put(root.path(), &drafter, BYTES);
    std::fs::write(
        root.path().join(SLUG).join("gemma-4-E2B-it-Q4_K_M.gguf"),
        b"model",
    )
    .expect("model");
    let found = canonical_model_file_in(root.path(), SLUG)
        .expect("scan")
        .expect("one model file");
    assert_eq!(found.filename, "gemma-4-E2B-it-Q4_K_M.gguf");
}

use std::time::{Duration, SystemTime};

use super::{remove_stale, safe_name, stage, STAGED_LIFETIME};
use crate::chat::attachments::MAX_ANY_BYTES;
use crate::error::HolziError;

#[test]
fn a_name_keeps_only_its_last_component() {
    assert_eq!(safe_name("../../etc/passwd"), "passwd");
    assert_eq!(safe_name("C:\\Users\\me\\photo.png"), "photo.png");
    assert_eq!(safe_name("image.png"), "image.png");
}

#[test]
fn a_name_without_anything_usable_falls_back() {
    assert_eq!(safe_name(""), "pasted");
    assert_eq!(safe_name("   "), "pasted");
    assert_eq!(safe_name(".."), "pasted");
    assert_eq!(safe_name("a/"), "pasted");
}

#[test]
fn characters_not_allowed_in_a_file_name_are_dropped() {
    assert_eq!(safe_name("a<b>c:d\"e|f?g*h.txt"), "abcdefgh.txt");
    assert_eq!(safe_name("line\nbreak.txt"), "linebreak.txt");
}

#[test]
fn a_paste_is_written_under_its_name_in_a_folder_of_its_own() {
    let root = tempfile::tempdir().expect("tempdir");
    let first = stage(root.path(), "image.png", b"one").expect("stage");
    let second = stage(root.path(), "image.png", b"two").expect("stage");

    assert_ne!(first, second);
    assert_eq!(
        first.file_name().and_then(|n| n.to_str()),
        Some("image.png")
    );
    assert_eq!(std::fs::read(&first).expect("read"), b"one");
    assert_eq!(std::fs::read(&second).expect("read"), b"two");
    assert!(first.starts_with(root.path()));
}

#[test]
fn a_name_cannot_lead_out_of_the_staging_folder() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = stage(root.path(), "../../escape.txt", b"x").expect("stage");
    assert!(path.starts_with(root.path()));
    assert_eq!(
        path.file_name().and_then(|n| n.to_str()),
        Some("escape.txt")
    );
}

#[test]
fn an_empty_paste_is_refused() {
    let root = tempfile::tempdir().expect("tempdir");
    let error = stage(root.path(), "image.png", b"").expect_err("empty");
    assert!(matches!(error, HolziError::InvalidInput { .. }));
}

#[test]
fn a_paste_beyond_the_largest_limit_is_refused() {
    let root = tempfile::tempdir().expect("tempdir");
    let bytes = vec![0u8; usize::try_from(MAX_ANY_BYTES).expect("fits") + 1];
    let error = stage(root.path(), "big.pdf", &bytes).expect_err("too large");
    assert!(matches!(error, HolziError::InvalidInput { .. }));
    assert_eq!(std::fs::read_dir(root.path()).expect("read_dir").count(), 0);
}

#[test]
fn only_folders_older_than_the_lifetime_are_removed() {
    let root = tempfile::tempdir().expect("tempdir");
    let kept = stage(root.path(), "new.png", b"x").expect("stage");

    let almost = SystemTime::now() + STAGED_LIFETIME - Duration::from_secs(60);
    remove_stale(root.path(), almost);
    assert!(kept.exists(), "a folder inside the lifetime stays");

    let later = SystemTime::now() + STAGED_LIFETIME + Duration::from_secs(60);
    remove_stale(root.path(), later);
    assert!(!kept.exists(), "a folder past the lifetime goes");
}

#[test]
fn a_missing_staging_folder_is_no_error_for_the_sweep() {
    let root = tempfile::tempdir().expect("tempdir");
    remove_stale(&root.path().join("not-there"), SystemTime::now());
}

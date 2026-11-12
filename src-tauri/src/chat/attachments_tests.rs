use std::io::Write;

use super::{
    classify_attachment, read_attachment_content, read_image_preview, usability_for,
    AttachmentUsability,
};
use crate::adapters::AttachmentKind;
use crate::files::test_support::{picked, PathOpener};
use crate::model_capabilities::ModelCapabilities;

fn write_temp_file(name: &str, bytes: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(name);
    let mut file = std::fs::File::create(&path).expect("create temp file");
    file.write_all(bytes).expect("write temp file");
    dir
}

#[test]
fn a_recognized_small_image_is_usable() {
    let bytes = b"not a real png but small";
    let dir = write_temp_file("photo.png", bytes);
    let info = classify_attachment(
        &PathOpener::default(),
        &picked(&dir.path().join("photo.png")),
    )
    .expect("classify");
    assert!(info.usable);
    assert!(info.reason.is_none());
    assert_eq!(info.size_bytes, bytes.len() as u64);
}

#[test]
fn an_unrecognized_extension_is_unusable_with_a_reason() {
    let dir = write_temp_file("archive.zip", b"PK\x03\x04");
    let info = classify_attachment(
        &PathOpener::default(),
        &picked(&dir.path().join("archive.zip")),
    )
    .expect("classify");
    assert!(!info.usable);
    assert!(info.reason.is_some());
}

#[test]
fn an_oversized_text_file_is_rejected_with_a_specific_reason() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("huge.txt");
    // Larger than the 5 MB text cap.
    let file = std::fs::File::create(&path).expect("create");
    file.set_len(6 * 1024 * 1024).expect("set_len");
    let info = classify_attachment(&PathOpener::default(), &picked(&path)).expect("classify");
    assert!(!info.usable);
    assert!(info.reason.as_deref().unwrap().contains("exceeds"));
    assert!(read_attachment_content(&PathOpener::default(), &picked(&path)).is_err());
}

#[test]
fn classify_errors_only_for_a_missing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("nope.png");
    assert!(classify_attachment(&PathOpener::default(), &picked(&missing)).is_err());
}

fn accepting(kinds: Vec<AttachmentKind>) -> ModelCapabilities {
    ModelCapabilities {
        accepted_attachment_kinds: Some(kinds),
        ..ModelCapabilities::default()
    }
}

#[test]
fn a_kind_in_the_determined_list_is_usable() {
    let caps = accepting(vec![AttachmentKind::Text, AttachmentKind::Image]);

    assert_eq!(
        usability_for(&AttachmentKind::Image, Some(&caps)),
        AttachmentUsability::Usable
    );
}

#[test]
fn a_kind_missing_from_the_determined_list_is_not_accepted() {
    let caps = accepting(vec![AttachmentKind::Text]);

    assert_eq!(
        usability_for(&AttachmentKind::Document, Some(&caps)),
        AttachmentUsability::NotAccepted
    );
    // An authoritative empty list (a local model) accepts nothing.
    assert_eq!(
        usability_for(&AttachmentKind::Text, Some(&accepting(Vec::new()))),
        AttachmentUsability::NotAccepted
    );
}

#[test]
fn undetermined_support_is_neither_usable_nor_reported_as_unsupported() {
    assert_eq!(
        usability_for(&AttachmentKind::Image, None),
        AttachmentUsability::Undetermined
    );
    assert_eq!(
        usability_for(&AttachmentKind::Image, Some(&ModelCapabilities::default())),
        AttachmentUsability::Undetermined
    );
}

#[test]
fn an_image_previews_with_its_bytes() {
    let dir = write_temp_file("photo.png", b"image bytes");
    let bytes = read_image_preview(
        &PathOpener::default(),
        &picked(&dir.path().join("photo.png")),
    )
    .expect("preview");
    assert_eq!(bytes, b"image bytes");
}

#[test]
fn a_file_that_is_no_image_has_no_preview() {
    let dir = write_temp_file("notes.txt", b"text");
    let error = read_image_preview(
        &PathOpener::default(),
        &picked(&dir.path().join("notes.txt")),
    )
    .expect_err("not an image");
    assert!(matches!(
        error,
        crate::error::HolziError::InvalidInput { .. }
    ));
}

#[test]
fn an_image_beyond_the_image_limit_has_no_preview() {
    let big = vec![0u8; 5 * 1024 * 1024 + 1];
    let dir = write_temp_file("big.png", &big);
    let error = read_image_preview(&PathOpener::default(), &picked(&dir.path().join("big.png")))
        .expect_err("too large");
    assert!(matches!(
        error,
        crate::error::HolziError::InvalidInput { .. }
    ));
}

#[test]
fn a_missing_image_has_no_preview() {
    let dir = tempfile::tempdir().expect("tempdir");
    let error = read_image_preview(
        &PathOpener::default(),
        &picked(&dir.path().join("gone.png")),
    )
    .expect_err("missing");
    assert!(matches!(
        error,
        crate::error::HolziError::InvalidInput { .. }
    ));
}

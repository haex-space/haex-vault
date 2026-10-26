//! Tests for attachments shown inside holzi (spec 058, research R2–R4): text the way the file
//! viewer reads it, PDFs as they are, the refusal of everything else, and the copies for the
//! system's app with names every system can create.

use super::attachment_view::{copy_file_name, pdf_bytes, remove_copy, text_of, write_copy};
use crate::error::HolziError;
use crate::files::local::text::TEXT_LIMIT;

fn reason(result: crate::error::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(HolziError::InvalidInput { reason }) => reason,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[test]
fn a_text_attachment_comes_back_as_written() {
    let text = text_of("notiz.txt", "Grüße\nZeile 2".as_bytes().to_vec()).unwrap();
    assert_eq!(text.text, "Grüße\nZeile 2");
    assert!(!text.truncated);
}

#[test]
fn invalid_utf8_becomes_the_replacement_character() {
    let text = text_of("notiz.md", vec![b'a', 0xff, b'b']).unwrap();
    assert_eq!(text.text, "a\u{fffd}b");
}

#[test]
fn a_text_longer_than_the_limit_is_cut_at_the_limit() {
    let limit = usize::try_from(TEXT_LIMIT).unwrap();
    let text = text_of("gross.log", vec![b'x'; limit + 10]).unwrap();
    assert_eq!(text.text.len(), limit);
    assert!(text.truncated);
}

#[test]
fn a_text_of_exactly_the_limit_is_whole() {
    let limit = usize::try_from(TEXT_LIMIT).unwrap();
    let text = text_of("genau.log", vec![b'x'; limit]).unwrap();
    assert_eq!(text.text.len(), limit);
    assert!(!text.truncated);
}

#[test]
fn an_empty_text_is_empty_and_whole() {
    let text = text_of("leer.txt", Vec::new()).unwrap();
    assert_eq!(text.text, "");
    assert!(!text.truncated);
}

#[test]
fn a_nul_byte_near_the_start_marks_a_binary_file() {
    assert_eq!(reason(text_of("kaputt.txt", b"a\0b".to_vec())), "binary");
}

#[test]
fn a_name_the_file_viewer_does_not_show_as_text_is_refused() {
    for name in ["bild.png", "police.pdf", "tabelle.ods", "ohne-endung"] {
        assert_eq!(reason(text_of(name, b"abc".to_vec())), "not_text", "{name}");
    }
}

#[test]
fn a_pdf_attachment_keeps_its_bytes() {
    for name in ["police.pdf", "Brief.PDF"] {
        assert_eq!(pdf_bytes(name, b"%PDF-1.4".to_vec()).unwrap(), b"%PDF-1.4");
    }
}

#[test]
fn a_name_that_is_no_pdf_is_refused() {
    for name in ["notiz.txt", "bild.png", "ohne-endung"] {
        assert_eq!(
            reason(pdf_bytes(name, b"%PDF".to_vec())),
            "not_pdf",
            "{name}"
        );
    }
}

#[test]
fn an_ordinary_name_stays_as_it_is() {
    assert_eq!(copy_file_name("notiz.txt"), "notiz.txt");
    assert_eq!(copy_file_name("Grüße 2026.pdf"), "Grüße 2026.pdf");
}

#[test]
fn characters_windows_refuses_become_underscores() {
    assert_eq!(copy_file_name("a:b?.txt"), "a_b_.txt");
    assert_eq!(copy_file_name(r#"x<y>|"*.md"#), "x_y____.md");
}

#[test]
fn a_name_never_climbs_out_of_its_folder() {
    let name = copy_file_name("../geheim");
    assert!(!name.contains('/') && !name.contains('\\'), "{name}");
}

#[test]
fn a_trailing_dot_or_space_becomes_an_underscore() {
    assert_eq!(copy_file_name("datei."), "datei_");
    assert_eq!(copy_file_name("datei "), "datei_");
}

#[test]
fn a_device_name_of_windows_gets_a_leading_underscore() {
    for (name, copy) in [
        ("CON.txt", "_CON.txt"),
        ("nul", "_nul"),
        ("Com1.log", "_Com1.log"),
        ("lpt9", "_lpt9"),
    ] {
        assert_eq!(copy_file_name(name), copy, "{name}");
    }
    for name in ["CONSOLE.txt", "com0.txt", "COM10.txt", "auxiliar.md"] {
        assert_eq!(copy_file_name(name), name, "{name}");
    }
}

#[test]
fn an_empty_name_becomes_file_and_a_long_one_is_cut() {
    assert_eq!(copy_file_name(""), "file");
    assert!(copy_file_name(&"a".repeat(300)).chars().count() <= 255);
    assert!(
        copy_file_name(&format!("CON{}", "a".repeat(300)))
            .chars()
            .count()
            <= 255
    );
}

#[tokio::test]
async fn a_copy_lies_in_a_folder_of_its_own_with_the_exact_bytes() {
    let root = tempfile::tempdir().unwrap();
    let first = write_copy(root.path(), "a:b.txt", b"geheim").await.unwrap();
    let second = write_copy(root.path(), "a:b.txt", b"anders").await.unwrap();
    assert_eq!(first.file_name().unwrap(), "a_b.txt");
    assert_eq!(first.parent().unwrap().parent().unwrap(), root.path());
    assert_ne!(first.parent(), second.parent());
    assert_eq!(std::fs::read(&first).unwrap(), b"geheim");
    assert_eq!(std::fs::read(&second).unwrap(), b"anders");
}

#[tokio::test]
async fn removing_a_copy_removes_its_folder() {
    let root = tempfile::tempdir().unwrap();
    let copy = write_copy(root.path(), "notiz.txt", b"x").await.unwrap();
    let dir = copy.parent().unwrap().to_owned();
    remove_copy(&copy).await;
    assert!(!dir.exists());
    assert!(root.path().exists());
}

#[tokio::test]
async fn a_copy_that_cannot_be_written_leaves_no_folder() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("root");
    std::fs::write(&root, b"a file where the folder should be").unwrap();
    assert_eq!(
        reason(write_copy(&root, "notiz.txt", b"x").await),
        "unwritable"
    );
    assert!(root.is_file());
    assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn the_sweep_of_spec_048_removes_every_copy() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("files-opened-encrypted");
    write_copy(&root, "a.txt", b"1").await.unwrap();
    write_copy(&root, "b.txt", b"2").await.unwrap();
    crate::files::encrypted::opened::sweep(&root);
    assert!(!root.exists());
}

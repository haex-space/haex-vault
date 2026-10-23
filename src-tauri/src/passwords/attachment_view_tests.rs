//! Tests for attachments shown inside holzi (spec 058, research R2): text the way the file viewer
//! reads it, and the refusal of everything that is not text.

use super::attachment_view::text_of;
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

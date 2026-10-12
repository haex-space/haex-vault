//! Text of PDFs for agents (research R8): the text layer, a scan without one, a file encrypted
//! without a user password, a broken one, a panic in the parser, and the limits of FR-035.

use super::{
    from_bytes, from_file, guarded, limited, Extracted, NOTE_SCANNED, NOTE_TOO_LARGE,
    NOTE_UNREADABLE, TEXT_CHARS,
};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/files/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture")
}

fn text(extracted: Extracted) -> String {
    match extracted {
        Extracted::Text { text, truncated } => {
            assert!(!truncated);
            text
        }
        Extracted::NoText(note) => panic!("no text: {note}"),
    }
}

#[test]
fn the_text_layer_is_read() {
    let text = text(from_bytes("steuer.pdf", &fixture("steuer.pdf")));
    assert!(text.contains("Steuerbescheid 2025"), "{text}");
    assert!(text.contains("312 Euro"), "{text}");
}

#[test]
fn a_file_encrypted_without_a_user_password_is_read() {
    let text = text(from_bytes("geschuetzt.pdf", &fixture("geschuetzt.pdf")));
    assert!(text.contains("Steuerbescheid 2025"), "{text}");
    assert!(text.contains("312 Euro"), "{text}");
}

#[test]
fn a_scan_without_text_gets_a_note() {
    assert_eq!(
        from_bytes("scan.pdf", &fixture("scan.pdf")),
        Extracted::NoText(NOTE_SCANNED)
    );
}

#[test]
fn a_broken_file_gets_a_note() {
    assert_eq!(
        from_bytes("kaputt.pdf", b"%PDF-1.4\nnichts als Rauschen"),
        Extracted::NoText(NOTE_UNREADABLE)
    );
}

#[test]
fn a_panic_in_a_parser_is_caught() {
    let result = guarded(|| panic!("a parser gave up"));
    assert_eq!(result, Extracted::NoText(NOTE_UNREADABLE));
}

#[test]
fn a_file_over_50_mb_is_not_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("riesig.pdf");
    let file = std::fs::File::create(&path).expect("create");
    file.set_len(50 * 1024 * 1024 + 1).expect("sparse");
    assert_eq!(
        from_file(&path, "riesig.pdf").expect("checked"),
        Extracted::NoText(NOTE_TOO_LARGE)
    );
}

#[test]
fn text_is_cut_at_the_limit() {
    match limited("ü".repeat(TEXT_CHARS + 3)) {
        Extracted::Text { text, truncated } => {
            assert!(truncated);
            assert_eq!(text.chars().count(), TEXT_CHARS);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        limited("kurz".to_owned()),
        Extracted::Text {
            text: "kurz".to_owned(),
            truncated: false
        }
    );
}

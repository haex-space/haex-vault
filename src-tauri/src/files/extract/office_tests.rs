//! Text of office documents for agents (research R8): docx and odt paragraph by paragraph,
//! spreadsheets sheet by sheet with their names, and broken files.

use std::io::Write;

use super::{from_bytes, Extracted, NOTE_FORMAT, NOTE_UNREADABLE};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/files/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture")
}

fn text(extracted: Extracted) -> String {
    match extracted {
        Extracted::Text { text, .. } => text,
        Extracted::NoText(note) => panic!("no text: {note}"),
    }
}

#[test]
fn word_and_writer_documents_keep_their_paragraphs() {
    for name in ["mietvertrag.docx", "mietvertrag.odt"] {
        let text = text(from_bytes(name, &fixture(name)));
        assert_eq!(
            text.trim(),
            "Mietvertrag Wohnung 3\nDie Miete beträgt 850 Euro im Monat.",
            "{name}"
        );
    }
}

#[test]
fn spreadsheets_name_each_sheet() {
    for name in ["tabelle.xlsx", "tabelle.ods"] {
        let text = text(from_bytes(name, &fixture(name)));
        let einnahmen = text.find("## Einnahmen").expect(name);
        let ausgaben = text.find("## Ausgaben").expect(name);
        assert!(einnahmen < ausgaben, "{name}: {text}");
        assert!(
            text.contains("Monat\tBetrag\nJanuar\t1200"),
            "{name}: {text}"
        );
        assert!(text.contains("Miete\t850"), "{name}: {text}");
    }
}

#[test]
fn entities_tabs_and_breaks_of_a_docx_are_text() {
    let mut bytes = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .expect("entry");
        zip.write_all(
            br#"<?xml version="1.0"?><w:document xmlns:w="w"><w:body>
<w:p><w:r><w:t>A &amp; B</w:t><w:tab/><w:t>&#228;</w:t><w:br/><w:t xml:space="preserve"> C</w:t></w:r></w:p>
<w:p><w:r><w:instrText>PAGE</w:instrText><w:t>D</w:t></w:r></w:p></w:body></w:document>"#,
        )
        .expect("write");
        zip.finish().expect("finish");
    }
    assert_eq!(text(from_bytes("x.docx", &bytes)), "A & B\tä\n C\nD\n");
}

#[test]
fn broken_documents_and_unknown_formats_get_a_note() {
    for name in ["x.docx", "x.odt", "x.xlsx", "x.ods"] {
        assert_eq!(
            from_bytes(name, b"PK not really a zip"),
            Extracted::NoText(NOTE_UNREADABLE),
            "{name}"
        );
    }
    assert_eq!(
        from_bytes("alt.doc", b"whatever"),
        Extracted::NoText(NOTE_FORMAT)
    );
}

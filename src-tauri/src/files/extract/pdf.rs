//! Text of a PDF with `pdf-extract` (research R8). A file encrypted without a user password is
//! opened with the empty one. A document whose pages hold almost no text has no text layer: a scan.
//!
//! pdf-extract 0.12 only knows the text operators `Tj`, `TJ` and `T*`; the "next line and show"
//! operators `'` and `"`, which Ghostscript writes for every line after the first, are spelled out
//! into those before the text is read, or all such lines would be missing.

use lopdf::content::{Content, Operation};
use lopdf::Document;
use pdf_extract::{output_doc_page, PlainTextOutput};

use super::{limited, Extracted, NOTE_SCANNED, NOTE_UNREADABLE};

/// Fewer visible characters per page than this: no text layer.
const SCANNED_PER_PAGE: usize = 16;

pub(super) fn text(bytes: &[u8]) -> Extracted {
    let pages = match pages(bytes) {
        Ok(pages) => pages,
        Err(error) => {
            log::info!("files: no text from a PDF: {error}");
            return Extracted::NoText(NOTE_UNREADABLE);
        }
    };
    let visible: usize = pages
        .iter()
        .map(|page| page.chars().filter(|c| !c.is_whitespace()).count())
        .sum();
    if visible < SCANNED_PER_PAGE * pages.len().max(1) {
        return Extracted::NoText(NOTE_SCANNED);
    }
    limited(pages.join("\n\n"))
}

fn pages(bytes: &[u8]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut doc = Document::load_mem(bytes)?;
    if doc.is_encrypted() {
        doc.decrypt("")?;
    }
    let page_ids: Vec<_> = doc.get_pages().into_values().collect();
    for page_id in &page_ids {
        let content = Content::decode(&doc.get_page_content(*page_id)?)?;
        if content
            .operations
            .iter()
            .any(|op| matches!(op.operator.as_str(), "'" | "\""))
        {
            let spelled = Content {
                operations: spell_out(content.operations),
            };
            doc.change_page_content(*page_id, spelled.encode()?)?;
        }
    }
    let mut pages = Vec::new();
    for number in 1..=u32::try_from(page_ids.len())? {
        let mut text = String::new();
        output_doc_page(&doc, &mut PlainTextOutput::new(&mut text), number)?;
        pages.push(text);
    }
    Ok(pages)
}

/// `(s) '` → `T* (s) Tj`; `aw ac (s) "` → `aw Tw ac Tc T* (s) Tj`.
fn spell_out(operations: Vec<Operation>) -> Vec<Operation> {
    let mut out = Vec::with_capacity(operations.len());
    for op in operations {
        match (op.operator.as_str(), op.operands.as_slice()) {
            ("'", [text]) => {
                out.push(Operation::new("T*", vec![]));
                out.push(Operation::new("Tj", vec![text.clone()]));
            }
            ("\"", [word, char_spacing, text]) => {
                out.push(Operation::new("Tw", vec![word.clone()]));
                out.push(Operation::new("Tc", vec![char_spacing.clone()]));
                out.push(Operation::new("T*", vec![]));
                out.push(Operation::new("Tj", vec![text.clone()]));
            }
            _ => out.push(op),
        }
    }
    out
}

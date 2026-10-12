//! Text of office documents (research R8): `word/document.xml` of a docx and `content.xml` of an
//! odt read with `zip` and `quick-xml`, one line per paragraph; spreadsheets with `calamine`, one
//! block per sheet under its name, cells separated by tabs.

use std::io::{Cursor, Read};

use calamine::Reader;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::Event;

use super::{limited, Extracted, INPUT_LIMIT, NOTE_UNREADABLE};

/// The elements of a document that carry text, end a line, or stand for a character.
struct Markup {
    /// Text counts only inside this element (`w:t`), or inside any paragraph when `None`.
    text: Option<&'static [u8]>,
    paragraphs: &'static [&'static [u8]],
    tab: &'static [u8],
    breaks: &'static [&'static [u8]],
    /// An element that stands for spaces (`text:s`, attribute `text:c` = count).
    space: Option<&'static [u8]>,
}

const WORD: Markup = Markup {
    text: Some(b"w:t"),
    paragraphs: &[b"w:p"],
    tab: b"w:tab",
    breaks: &[b"w:br", b"w:cr"],
    space: None,
};

const WRITER: Markup = Markup {
    text: None,
    paragraphs: &[b"text:p", b"text:h"],
    tab: b"text:tab",
    breaks: &[b"text:line-break"],
    space: Some(b"text:s"),
};

pub(super) fn word(bytes: &[u8]) -> Extracted {
    xml_text(bytes, "word/document.xml", &WORD)
}

pub(super) fn writer(bytes: &[u8]) -> Extracted {
    xml_text(bytes, "content.xml", &WRITER)
}

/// The text of the XML part `part` of the ZIP `bytes`.
fn xml_text(bytes: &[u8], part: &str, markup: &Markup) -> Extracted {
    let Some(xml) = zip_part(bytes, part) else {
        return Extracted::NoText(NOTE_UNREADABLE);
    };
    match paragraphs(&xml, markup) {
        Some(text) => limited(text),
        None => Extracted::NoText(NOTE_UNREADABLE),
    }
}

fn zip_part(bytes: &[u8], part: &str) -> Option<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let entry = archive.by_name(part).ok()?;
    let mut xml = Vec::new();
    entry.take(INPUT_LIMIT).read_to_end(&mut xml).ok()?;
    Some(xml)
}

fn paragraphs(xml: &[u8], markup: &Markup) -> Option<String> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut out = String::new();
    // Inside the element text counts in.
    let mut depth = 0usize;
    let counts = |name: &[u8]| match markup.text {
        Some(text) => name == text,
        None => markup.paragraphs.contains(&name),
    };
    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Start(element) => {
                if counts(element.name().as_ref()) {
                    depth += 1;
                }
            }
            Event::End(element) => {
                let name = element.name();
                if counts(name.as_ref()) {
                    depth = depth.saturating_sub(1);
                }
                if markup.paragraphs.contains(&name.as_ref()) {
                    out.push('\n');
                }
            }
            Event::Empty(element) => {
                let name = element.name();
                let name = name.as_ref();
                if name == markup.tab {
                    out.push('\t');
                } else if markup.breaks.contains(&name) || markup.paragraphs.contains(&name) {
                    out.push('\n');
                } else if Some(name) == markup.space {
                    let count = element
                        .try_get_attribute("text:c")
                        .ok()
                        .flatten()
                        .and_then(|c| std::str::from_utf8(&c.value).ok()?.parse().ok())
                        .unwrap_or(1usize);
                    out.extend(std::iter::repeat_n(' ', count.min(1_000)));
                }
            }
            Event::Text(text) if depth > 0 => out.push_str(&text.decode().ok()?),
            Event::GeneralRef(reference) if depth > 0 => {
                if let Some(c) = reference.resolve_char_ref().ok()? {
                    out.push(c);
                } else if let Some(entity) = resolve_xml_entity(&reference.decode().ok()?) {
                    out.push_str(entity);
                }
            }
            Event::Eof => return Some(out),
            _ => {}
        }
        buf.clear();
    }
}

/// Every sheet under `## <name>`, its rows one per line, cells separated by tabs.
pub(super) fn sheets(bytes: &[u8]) -> Extracted {
    let Ok(mut workbook) = calamine::open_workbook_auto_from_rs(Cursor::new(bytes)) else {
        return Extracted::NoText(NOTE_UNREADABLE);
    };
    let mut out = String::new();
    for name in workbook.sheet_names() {
        let Ok(range) = workbook.worksheet_range(&name) else {
            continue;
        };
        out.push_str(&format!("## {name}\n"));
        for row in range.rows() {
            let cells: Vec<String> = row.iter().map(ToString::to_string).collect();
            out.push_str(cells.join("\t").trim_end());
            out.push('\n');
        }
        out.push('\n');
    }
    limited(out)
}

//! What agents get from files (spec 044 FR-035, FR-035a, research R2, R8): text from PDFs with a
//! text layer, Word and Writer documents and spreadsheets, and images sized for a model
//! ([`image`]). Input up to [`INPUT_LIMIT`], output up to
//! [`TEXT_CHARS`] characters; a parser that fails or panics gives a note instead of text, never an
//! error the agent cannot act on.

pub mod image;
mod office;
mod pdf;

use std::io::Read;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use crate::files::kind::extension;
use crate::files::local::ops::io_error;
use crate::files::FilesError;

/// The largest file text is taken from (FR-035).
pub const INPUT_LIMIT: u64 = 50 * 1024 * 1024;

/// The most characters of text an agent gets (FR-035).
pub const TEXT_CHARS: usize = 200_000;

pub const NOTE_SCANNED: &str =
    "This PDF has no text layer, probably a scan; holzi cannot read text from it.";
pub const NOTE_TOO_LARGE: &str =
    "The file is larger than 50 MB; holzi reads text only from smaller files.";
pub const NOTE_UNREADABLE: &str =
    "holzi could not read this document; it may be damaged or protected by a password.";
pub const NOTE_FORMAT: &str = "holzi cannot get the text out of this format.";

/// What a document gives an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extracted {
    Text {
        text: String,
        truncated: bool,
    },
    /// Why there is no text.
    NoText(&'static str),
}

/// The formats text is taken from.
enum Format {
    Pdf,
    Word,
    Writer,
    Sheet,
}

fn format_of(name: &str) -> Option<Format> {
    match extension(name).as_str() {
        "pdf" => Some(Format::Pdf),
        "docx" => Some(Format::Word),
        "odt" => Some(Format::Writer),
        "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" => Some(Format::Sheet),
        _ => None,
    }
}

/// Whether text is taken from a document of this name.
pub fn supports(name: &str) -> bool {
    format_of(name).is_some()
}

/// The text of the document `name` held in `bytes`.
pub fn from_bytes(name: &str, bytes: &[u8]) -> Extracted {
    let Some(format) = format_of(name) else {
        return Extracted::NoText(NOTE_FORMAT);
    };
    guarded(|| match format {
        Format::Pdf => pdf::text(bytes),
        Format::Word => office::word(bytes),
        Format::Writer => office::writer(bytes),
        Format::Sheet => office::sheets(bytes),
    })
}

/// The text of the document at `path`; a file over [`INPUT_LIMIT`] is not read.
pub fn from_file(path: &Path, name: &str) -> Result<Extracted, FilesError> {
    let size = std::fs::metadata(path)
        .map_err(|error| io_error(error, path))?
        .len();
    if size > INPUT_LIMIT {
        return Ok(Extracted::NoText(NOTE_TOO_LARGE));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(INPUT_LIMIT).read_to_end(&mut bytes))
        .map_err(|error| io_error(error, path))?;
    Ok(from_bytes(name, &bytes))
}

/// Runs a parser; a panic in it becomes [`NOTE_UNREADABLE`] (research R8: broken PDFs can panic).
pub(crate) fn guarded(work: impl FnOnce() -> Extracted) -> Extracted {
    catch_unwind(AssertUnwindSafe(work)).unwrap_or(Extracted::NoText(NOTE_UNREADABLE))
}

/// `text` cut at [`TEXT_CHARS`].
pub fn limited(mut text: String) -> Extracted {
    let cut = text.char_indices().nth(TEXT_CHARS).map(|(at, _)| at);
    if let Some(at) = cut {
        text.truncate(at);
    }
    Extracted::Text {
        text,
        truncated: cut.is_some(),
    }
}

#[cfg(test)]
#[path = "pdf_tests.rs"]
mod pdf_tests;

#[cfg(test)]
#[path = "office_tests.rs"]
mod office_tests;

#[cfg(test)]
#[path = "image_tests.rs"]
mod image_tests;

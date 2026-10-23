//! Attachments shown inside holzi (spec 058, research R2): a text attachment goes through the text
//! reader of the file viewer (spec 044 FR-015), so the limit of 5 MB, the detection of binary files
//! and the replacement of invalid UTF-8 are the same in both apps. Which attachment counts as text
//! is decided by [`viewer_kind`], never by a list of its own (FR-002).

use crate::error::{HolziError, Result};
use crate::files::kind::{viewer_kind, ViewerKind};
use crate::files::local::text::{text_from, TextContent, TEXT_LIMIT};
use crate::files::FilesErrorCode;

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_string(),
    }
}

/// The text of an attachment for the attachment view: `not_text` for a name the file viewer does
/// not show as text, `binary` for a file it detects as binary.
pub fn text_of(file_name: &str, bytes: Vec<u8>) -> Result<TextContent> {
    if viewer_kind(file_name) != ViewerKind::Text {
        return Err(invalid("not_text"));
    }
    text_from(bytes, TEXT_LIMIT).map_err(|error| match error.code {
        FilesErrorCode::Binary => invalid("binary"),
        _ => invalid("unreadable"),
    })
}

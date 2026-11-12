//! File attachments a user adds to the message being composed (spec
//! 011-composer-toolbar-parity). Classification is by file extension and
//! byte size only — never by parsing file contents, so a corrupt or
//! disguised file fails at the provider's own decode step, not here.
//!
//! Two separate concerns, kept apart because they answer different
//! questions (data-model.md "Message Attachment"): [`classify_attachment`]
//! ("is this file even attachable at all" — FR-016, independent of any
//! backend) and [`usability_for`] ("can the *currently selected*
//! model/backend use it" — FR-015, independent of the file itself).

use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::adapters::{Attachment, AttachmentKind};
use crate::error::{HolziError, Result};
use crate::files::picked::{self, Opener, PickedFile};
use crate::model_capabilities::ModelCapabilities;

/// Anthropic's own published per-content-type limits (research.md §4) —
/// reused rather than inventing holzi-specific numbers.
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TEXT_BYTES: u64 = 5 * 1024 * 1024;

fn max_bytes_for(kind: &AttachmentKind) -> u64 {
    match kind {
        AttachmentKind::Image => MAX_IMAGE_BYTES,
        AttachmentKind::Document => MAX_DOCUMENT_BYTES,
        AttachmentKind::Text => MAX_TEXT_BYTES,
    }
}

/// Extension-based kind + media-type detection from the file's shown name
/// (spec 043: a chosen file on Android has no path). `None` for anything not
/// in this list — an unrecognized extension is an unsupported type (FR-016),
/// not a guess.
fn kind_and_media_type(name: &str) -> Option<(AttachmentKind, &'static str)> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => (AttachmentKind::Image, "image/png"),
        "jpg" | "jpeg" => (AttachmentKind::Image, "image/jpeg"),
        "webp" => (AttachmentKind::Image, "image/webp"),
        "gif" => (AttachmentKind::Image, "image/gif"),
        "pdf" => (AttachmentKind::Document, "application/pdf"),
        "txt" => (AttachmentKind::Text, "text/plain"),
        "md" => (AttachmentKind::Text, "text/markdown"),
        _ => return None,
    })
}

fn human_bytes(bytes: u64) -> String {
    format!("{} MB", bytes / (1024 * 1024))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub name: String,
    pub size_bytes: u64,
    /// `None` when the file's extension isn't a supported attachment type
    /// at all (`usable` is then always `false`).
    pub kind: Option<AttachmentKindWire>,
    pub usable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKindWire {
    Image,
    Document,
    Text,
}

impl From<&AttachmentKind> for AttachmentKindWire {
    fn from(kind: &AttachmentKind) -> Self {
        match kind {
            AttachmentKind::Image => AttachmentKindWire::Image,
            AttachmentKind::Document => AttachmentKindWire::Document,
            AttachmentKind::Text => AttachmentKindWire::Text,
        }
    }
}

impl From<AttachmentKindWire> for AttachmentKind {
    fn from(kind: AttachmentKindWire) -> Self {
        match kind {
            AttachmentKindWire::Image => AttachmentKind::Image,
            AttachmentKindWire::Document => AttachmentKind::Document,
            AttachmentKindWire::Text => AttachmentKind::Text,
        }
    }
}

/// The largest limit of any kind: an attachment of unknown size is never
/// read further than this to find out whether it is too large.
pub const MAX_ANY_BYTES: u64 = MAX_DOCUMENT_BYTES;

fn cannot_read(name: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: format!("cannot read attachment {name}"),
    }
}

/// The size of a chosen file: from its metadata where it is a plain file,
/// otherwise by reading at most one byte past [`MAX_ANY_BYTES`] (a document
/// provider may hand out a stream).
fn size_of(opener: &impl Opener, file: &PickedFile, name: &str) -> Result<u64> {
    let source = picked::open_read(opener, file).map_err(|_| cannot_read(name))?;
    if let Some(metadata) = source.metadata().ok().filter(std::fs::Metadata::is_file) {
        return Ok(metadata.len());
    }
    std::io::copy(&mut source.take(MAX_ANY_BYTES + 1), &mut std::io::sink())
        .map_err(|_| cannot_read(name))
}

/// "Is this file even attachable at all" — type + size, independent of any
/// backend (FR-016). Errors only when the file cannot be read at all
/// (contracts/tauri-commands.md `inspect_attachment`); an oversized or
/// unsupported-type file still resolves normally with `usable: false`.
pub fn classify_attachment(opener: &impl Opener, file: &PickedFile) -> Result<AttachmentInfo> {
    let name = picked::display_name(opener, file);
    let size_bytes = size_of(opener, file, &name)?;

    let Some((kind, _media_type)) = kind_and_media_type(&name) else {
        return Ok(AttachmentInfo {
            name,
            size_bytes,
            kind: None,
            usable: false,
            reason: Some("unsupported file type".to_string()),
        });
    };
    let cap = max_bytes_for(&kind);
    if size_bytes > cap {
        return Ok(AttachmentInfo {
            name,
            size_bytes,
            kind: Some((&kind).into()),
            usable: false,
            reason: Some(format!(
                "exceeds the {} limit for this file type",
                human_bytes(cap)
            )),
        });
    }
    Ok(AttachmentInfo {
        name,
        size_bytes,
        kind: Some((&kind).into()),
        usable: true,
        reason: None,
    })
}

/// Whether the selected model can use an attachment of a given kind, decided
/// from its cached capabilities (spec 012), never from its provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentUsability {
    /// The model's determined accepted kinds include this one.
    Usable,
    /// The model's determined accepted kinds do not include this one.
    NotAccepted,
    /// Attachment support has not been determined for this model (no row, or
    /// the provider has not been asked yet) — not the same as unsupported.
    Undetermined,
}

/// "Can the currently selected model use a file of this kind" — FR-015,
/// independent of the file itself.
pub fn usability_for(
    kind: &AttachmentKind,
    capabilities: Option<&ModelCapabilities>,
) -> AttachmentUsability {
    match capabilities.and_then(|c| c.accepted_attachment_kinds.as_ref()) {
        Some(kinds) if kinds.contains(kind) => AttachmentUsability::Usable,
        Some(_) => AttachmentUsability::NotAccepted,
        None => AttachmentUsability::Undetermined,
    }
}

/// Re-reads the file at send time (FR-018 — a file can vanish or change
/// between attach and send). Errors here mean the caller must exclude this
/// one attachment from the send, not fail the whole message.
pub fn read_attachment_content(opener: &impl Opener, file: &PickedFile) -> Result<Attachment> {
    let name = picked::display_name(opener, file);
    let (kind, media_type) =
        kind_and_media_type(&name).ok_or_else(|| HolziError::InvalidInput {
            reason: format!("unsupported attachment type: {name}"),
        })?;
    let cap = max_bytes_for(&kind);
    let bytes = picked::read(opener, file, cap).map_err(|_| HolziError::InvalidInput {
        reason: format!("failed to read attachment {name}"),
    })?;
    if bytes.len() as u64 > cap {
        return Err(HolziError::InvalidInput {
            reason: format!(
                "attachment {name} exceeds the {} limit for this file type",
                human_bytes(cap)
            ),
        });
    }
    Ok(Attachment {
        name,
        kind,
        media_type: media_type.to_string(),
        bytes,
    })
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;

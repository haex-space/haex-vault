//! Attachments shown inside holzi (spec 058, research R2): a text attachment goes through the text
//! reader of the file viewer (spec 044 FR-015), so the limit of 5 MB, the detection of binary files
//! and the replacement of invalid UTF-8 are the same in both apps. A PDF goes to the media server
//! as it is (research R3). Which attachment counts as text or PDF is decided by [`viewer_kind`],
//! never by a list of its own (FR-002). For the system's app an attachment becomes a decrypted copy
//! in a folder of its own below the place of spec 048 (research R4), under a name every system can
//! create; holzi removes it when opening fails, when the vault closes and after a crash.

use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

use crate::error::{HolziError, Result};
use crate::files::kind::{viewer_kind, ViewerKind};
use crate::files::local::edit::FORBIDDEN;
use crate::files::local::text::{text_from, TextContent, TEXT_LIMIT};
use crate::files::FilesErrorCode;
use crate::passwords::binaries::sanitize_file_name;

/// The device preference that holds "Nicht mehr fragen" for attachments (FR-010).
pub const OPEN_ACK_KEY: &str = "passwords.attachmentOpenAck";

/// The most characters of a copy's name.
const MAX_NAME: usize = 255;

/// The names Windows keeps for devices, whatever the extension.
const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

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

/// The bytes of a PDF attachment for the media server: `not_pdf` for any other name.
pub fn pdf_bytes(file_name: &str, bytes: Vec<u8>) -> Result<Vec<u8>> {
    if viewer_kind(file_name) != ViewerKind::Pdf {
        return Err(invalid("not_pdf"));
    }
    Ok(bytes)
}

/// The name of the copy for the system's app: the stored name as text (no separators, no control
/// characters), then the characters and endings Windows refuses as `_`, and a device name of Windows
/// (`CON`, `COM1`, …) with a leading `_`, so the copy can be created on every system.
pub fn copy_file_name(name: &str) -> String {
    let mut cleaned: String = sanitize_file_name(name)
        .chars()
        .map(|c| if FORBIDDEN.contains(&c) { '_' } else { c })
        .collect();
    if cleaned.ends_with(['.', ' ']) {
        cleaned.pop();
        cleaned.push('_');
    }
    let stem = cleaned.split('.').next().unwrap_or_default();
    if is_reserved(stem) {
        cleaned.insert(0, '_');
    }
    cleaned.chars().take(MAX_NAME).collect()
}

fn is_reserved(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    if RESERVED.contains(&upper.as_str()) {
        return true;
    }
    match upper.as_bytes() {
        [b'C', b'O', b'M', digit] | [b'L', b'P', b'T', digit] => (b'1'..=b'9').contains(digit),
        _ => false,
    }
}

/// Writes `bytes` as `<root>/<new uuid>/<copy_file_name(file_name)>` and returns its path. On any
/// failure the new folder goes again and the answer is `unwritable`.
pub async fn write_copy(root: &Path, file_name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let dir = root.join(uuid::Uuid::new_v4().to_string());
    let target = dir.join(copy_file_name(file_name));
    let written = async {
        tokio::fs::create_dir_all(&dir).await?;
        let mut file = tokio::fs::File::create(&target).await?;
        file.write_all(bytes).await?;
        file.sync_all().await
    }
    .await;
    if written.is_err() {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(invalid("unwritable"));
    }
    Ok(target)
}

/// Removes a copy made by [`write_copy`] with its folder.
pub async fn remove_copy(path: &Path) {
    let Some(dir) = path.parent() else { return };
    if let Err(error) = tokio::fs::remove_dir_all(dir).await {
        log::warn!(
            "passwords: a copy for the system app could not be removed: {}",
            error.kind()
        );
    }
}

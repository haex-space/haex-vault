//! Media types and viewer kinds by file name (spec 044 FR-009, FR-014, research R13). The window
//! mirrors [`viewer_kind`] in `src/lib/files/viewerKind.ts`; `scripts/check-files-viewer.ts` keeps
//! both tables equal.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How the viewer shows a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum ViewerKind {
    Text,
    Pdf,
    Image,
    Video,
    Audio,
    /// Name, type, size, date and "open with the system app" (FR-014).
    Info,
}

/// Whether the viewer shows `kind` from the media server (images, video, audio, PDF); text is read
/// and the rest goes to the info view.
pub fn streams(kind: ViewerKind) -> bool {
    matches!(
        kind,
        ViewerKind::Image | ViewerKind::Video | ViewerKind::Audio | ViewerKind::Pdf
    )
}

/// The lower-case extension of `name`, without the dot; empty when there is none. A leading dot
/// alone (`.bashrc`) is a hidden name, not an extension.
pub fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// The media type of a file by its name; `None` when unknown.
pub fn mime_for(name: &str) -> Option<&'static str> {
    Some(match extension(name).as_str() {
        "txt" | "text" | "log" | "md" | "markdown" | "ini" | "conf" | "cfg" | "toml" | "yaml"
        | "yml" | "rs" | "ts" | "js" | "mjs" | "vue" | "py" | "sh" | "c" | "h" | "cpp" | "java"
        | "kt" | "go" | "rb" | "php" | "sql" | "env" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "heic" | "heif" => "image/heic",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/aac",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "zip" => "application/zip",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        _ => return None,
    })
}

/// How the viewer shows a file by its name. Formats a web view rarely shows (HEIC, MKV) still get
/// their kind; a failed playback falls back to the info view in the window (FR-014).
pub fn viewer_kind(name: &str) -> ViewerKind {
    let Some(mime) = mime_for(name) else {
        return ViewerKind::Info;
    };
    if mime == "application/pdf" {
        ViewerKind::Pdf
    } else if mime == "image/heic" {
        ViewerKind::Info
    } else if mime.starts_with("image/") {
        ViewerKind::Image
    } else if mime.starts_with("video/") {
        ViewerKind::Video
    } else if mime.starts_with("audio/") {
        ViewerKind::Audio
    } else if mime.starts_with("text/") || mime == "application/json" || mime == "application/xml" {
        ViewerKind::Text
    } else {
        ViewerKind::Info
    }
}

/// What a filter looks for (spec 044 FR-029). The window mirrors [`category`] in
/// `src/lib/files/filters.ts`; `scripts/check-files-filters.ts` runs the same cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FileCategory {
    Image,
    Video,
    Audio,
    Document,
    Text,
}

/// Extensions of documents: PDF and office formats, including those haex-vault does not show.
const DOCUMENTS: &[&str] = &[
    "pdf", "doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "ppt", "pptx", "odp", "epub",
];

/// The category of a file by its name; `None` for anything else (archives, programs, …).
pub fn category(name: &str) -> Option<FileCategory> {
    if DOCUMENTS.contains(&extension(name).as_str()) {
        return Some(FileCategory::Document);
    }
    match viewer_kind(name) {
        ViewerKind::Image => Some(FileCategory::Image),
        ViewerKind::Video => Some(FileCategory::Video),
        ViewerKind::Audio => Some(FileCategory::Audio),
        ViewerKind::Text => Some(FileCategory::Text),
        // A photo from a phone is an image, even where the web view cannot show it.
        ViewerKind::Info if mime_for(name) == Some("image/heic") => Some(FileCategory::Image),
        _ => None,
    }
}

#[cfg(test)]
#[path = "kind_tests.rs"]
mod tests;

//! An image or file pasted from the clipboard (spec 011-composer-toolbar-parity, FR-019). The
//! webview hands over the bytes, not a path, so they are written to a file of their own in the
//! app's cache folder and then attached like any chosen file: the usual classification, size
//! check and send-time read apply unchanged. The cache folder lies inside the folders a path may
//! come from on every platform (`files::picked::Opener::path_roots`).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tauri::{AppHandle, Manager};

use crate::chat::attachments::MAX_ANY_BYTES;
use crate::error::{HolziError, Result};

/// Name of the folder inside the app's cache folder that holds the staged files.
const STAGING_FOLDER: &str = "pasted-attachments";

/// A staged file is only needed until the message is sent; a day is far longer than composing one.
const STAGED_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

/// The name shown for a paste that arrives without a usable one.
const FALLBACK_NAME: &str = "pasted";

/// The name the attachment is listed under: the last component only, without anything that is not
/// allowed in a file name on some platform, so a name cannot lead out of its folder.
pub fn safe_name(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        .collect();
    let cleaned = cleaned.trim().trim_matches('.');
    if cleaned.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        cleaned.to_string()
    }
}

/// Writes `bytes` to `<root>/<fresh id>/<name>` and returns that path. Every paste gets its own
/// folder, so two pastes with the same name (a clipboard image is always `image.png`) never
/// overwrite each other. Folders older than a day are removed first.
pub fn stage(root: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    if bytes.is_empty() {
        return Err(HolziError::InvalidInput {
            reason: "the pasted file is empty".to_string(),
        });
    }
    if bytes.len() as u64 > MAX_ANY_BYTES {
        return Err(HolziError::InvalidInput {
            reason: format!(
                "the pasted file is larger than the {} MB an attachment may have",
                MAX_ANY_BYTES / (1024 * 1024)
            ),
        });
    }
    remove_stale(root, SystemTime::now());
    let folder = root.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(safe_name(name));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Removes the staged folders last changed more than [`STAGED_LIFETIME`] before `now`. A folder
/// that cannot be removed is left for the next paste; it is no reason to refuse this one.
pub fn remove_stale(root: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STAGED_LIFETIME);
        if stale {
            if let Err(error) = std::fs::remove_dir_all(entry.path()) {
                log::warn!("pasted attachment: removing a stale folder failed: {error}");
            }
        }
    }
}

/// The file name travels in this header, percent-encoded, because the body is the raw bytes.
const NAME_HEADER: &str = "x-file-name";

/// Saves a pasted file and returns the path to attach (the body is the file's bytes, the header
/// [`NAME_HEADER`] its name). The window then adds that path like one chosen in the dialog.
#[tauri::command]
pub async fn stage_pasted_attachment(
    app: AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(HolziError::InvalidInput {
            reason: "the pasted file must arrive as raw bytes".to_string(),
        });
    };
    let name = request
        .headers()
        .get(NAME_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(|value| percent_encoding::percent_decode_str(value).decode_utf8_lossy())
        .unwrap_or_default()
        .into_owned();
    let root = staging_root(&app);
    let bytes = bytes.clone();
    let path = tauri::async_runtime::spawn_blocking(move || stage(&root, &name, &bytes))
        .await
        .map_err(|e| HolziError::Io {
            reason: format!("stage_pasted_attachment join: {e}"),
        })??;
    Ok(path.to_string_lossy().into_owned())
}

/// The folder the staged files go to.
pub fn staging_root(app: &AppHandle) -> PathBuf {
    app.path().app_cache_dir().map_or_else(
        |_| std::env::temp_dir().join("holzi-pasted-attachments"),
        |cache| cache.join(STAGING_FOLDER),
    )
}

#[cfg(test)]
#[path = "pasted_attachments_tests.rs"]
mod tests;

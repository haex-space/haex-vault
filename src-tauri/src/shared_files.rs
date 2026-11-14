//! Files another app shared to holzi (spec 092, FR-037, FR-035, `contracts/android-intents.md`): a
//! pkpass, a PDF or a picture from the share sheet of Android. The Kotlin side copies each shared
//! stream at once into the app's own cache directory (the read permission of the address is
//! transient) and reports the copy; this store holds the copies until the window asks for them,
//! which is after a vault is open (spec 013: a cold process starts, the vault is locked, the person
//! unlocks, then the dialog opens). Nothing is read or imported here, and a copy is removed as soon
//! as the window is done with it: the file never leaves the device and stays no longer than needed.
//!
//! The window sees only paths inside [`DIRECTORY`] of the cache directory; the commands refuse and
//! never delete anything else.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_holzi_android::HolziAndroidExt;
use ts_rs::TS;

use crate::error::{HolziError, Result};

/// The directory below the app's cache directory where the copies are made.
pub const DIRECTORY: &str = "shared-files";
/// The event that tells the window that files wait.
pub const EVENT: &str = "shared-files-changed";
/// The largest file taken, as for an attachment (spec 034 FR-020).
pub const MAX_BYTES: u64 = 25 * 1024 * 1024;
/// The most files kept waiting; a share sheet hands over a few.
pub const MAX_WAITING: usize = 20;
/// The longest name shown.
const MAX_NAME_CHARS: usize = 120;

/// What a shared file is, which decides the dialog that opens for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SharedKind {
    /// A pass file: the import dialog.
    Pkpass,
    /// A PDF: the capture dialog reads its pages.
    Pdf,
    /// A picture: the capture dialog reads it.
    Image,
}

impl SharedKind {
    /// The kind of a file by the type its sender named, else by the ending of its name; `None` for
    /// anything the share target did not offer to take.
    pub fn of(mime: &str, name: &str) -> Option<Self> {
        let mime = mime.trim().to_ascii_lowercase();
        let ending = name
            .rsplit_once('.')
            .map(|(_, ending)| ending.to_ascii_lowercase())
            .unwrap_or_default();
        if mime == "application/vnd.apple.pkpass" || ending == "pkpass" {
            Some(Self::Pkpass)
        } else if mime == "application/pdf" || ending == "pdf" {
            Some(Self::Pdf)
        } else if mime.starts_with("image/")
            || matches!(ending.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif")
        {
            Some(Self::Image)
        } else {
            None
        }
    }
}

/// A copy the Kotlin side made and reports.
#[derive(Debug, Clone, Deserialize)]
pub struct Incoming {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub mime: String,
}

/// A waiting file as the window sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct SharedFileView {
    pub path: String,
    pub name: String,
    pub kind: SharedKind,
}

/// The files that wait for a vault to be open.
pub struct SharedFiles {
    dir: PathBuf,
    waiting: Mutex<Vec<SharedFileView>>,
}

impl SharedFiles {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            waiting: Mutex::new(Vec::new()),
        }
    }

    /// Whether `path` is a file directly inside the directory of the copies: absolute, no `..`,
    /// and (after resolving links) really there.
    fn inside(&self, path: &Path) -> bool {
        if !path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return false;
        }
        let Ok(root) = self.dir.canonicalize() else {
            return false;
        };
        // A copy that is gone already still counts as one (a second discard is fine): its place
        // is that of its directory.
        let resolved = path.canonicalize().ok().or_else(|| {
            let parent = path.parent()?.canonicalize().ok()?;
            Some(parent.join(path.file_name()?))
        });
        resolved.is_some_and(|resolved| resolved.parent() == Some(root.as_path()))
    }

    /// Removes `path` if it is a copy; anything else is left alone.
    fn remove(&self, path: &Path) {
        if !self.inside(path) {
            return;
        }
        if let Err(error) = fs::remove_file(path) {
            if error.kind() != ErrorKind::NotFound {
                log::warn!("shared files: a copy was not removed: {error}");
            }
        }
    }

    /// Takes a copy into the waiting list. A copy that is not a plain file of the directory, is too
    /// large, is of a kind the share target does not take or does not fit the list is removed and
    /// refused (`None`).
    pub fn park(&self, incoming: Incoming) -> Option<SharedFileView> {
        let path = PathBuf::from(&incoming.path);
        let plain = self.inside(&path)
            && fs::symlink_metadata(&path)
                .is_ok_and(|meta| meta.is_file() && meta.len() <= MAX_BYTES);
        let name = display_name(&incoming.name);
        let kind = SharedKind::of(&incoming.mime, &name);
        let mut waiting = self.waiting.lock().unwrap_or_else(PoisonError::into_inner);
        let (true, Some(kind), true) = (plain, kind, waiting.len() < MAX_WAITING) else {
            drop(waiting);
            self.remove(&path);
            return None;
        };
        let view = SharedFileView {
            path: incoming.path,
            name,
            kind,
        };
        waiting.push(view.clone());
        Some(view)
    }

    /// Every waiting file; none waits afterwards (the window owns them and removes each copy when
    /// it is done, [`SharedFiles::discard`]).
    pub fn take_all(&self) -> Vec<SharedFileView> {
        std::mem::take(&mut *self.waiting.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Removes a copy, whether it still waits or the window has taken it. A path that is no copy
    /// is refused.
    pub fn discard(&self, path: &str) -> Result<()> {
        let path = Path::new(path);
        if !self.inside(path) {
            return Err(HolziError::InvalidInput {
                reason: "not a shared file".to_owned(),
            });
        }
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|file| Path::new(&file.path) != path);
        self.remove(path);
        Ok(())
    }
}

/// A name for the window: the last part, no control characters, not too long; `File` if nothing is
/// left.
fn display_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = last
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_CHARS)
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        "File".to_owned()
    } else {
        clean.to_owned()
    }
}

/// Starts listening to the plugin: a copy it reports waits here and the window is told. On a
/// desktop nothing ever arrives.
pub fn watch<R: Runtime>(app: &AppHandle<R>) {
    let dir = match app.path().app_cache_dir() {
        Ok(cache) => cache.join(DIRECTORY),
        Err(error) => {
            log::warn!("shared files: no cache directory: {error}");
            return;
        }
    };
    app.manage(SharedFiles::new(dir));
    let handle = app.clone();
    let watched = app.holzi_android().watch_shared_files(move |json| {
        let Ok(incoming) = serde_json::from_str::<Incoming>(&json) else {
            log::warn!("shared files: a report could not be read");
            return;
        };
        let Some(store) = handle.try_state::<SharedFiles>() else {
            return;
        };
        if store.park(incoming).is_some() {
            if let Err(error) = handle.emit(EVENT, ()) {
                log::warn!("shared files: {error}");
            }
        }
    });
    if let Err(error) = watched {
        log::warn!("shared files: not watched: {error}");
    }
}

/// The files that wait, for a window whose vault is open; none wait afterwards.
#[tauri::command]
pub fn shared_files_take(state: State<'_, SharedFiles>) -> Vec<SharedFileView> {
    state.take_all()
}

/// Removes a copy once the window has read it or the person has let it go (FR-035).
#[tauri::command]
pub fn shared_files_discard(state: State<'_, SharedFiles>, path: String) -> Result<()> {
    state.discard(&path)
}

#[cfg(test)]
#[path = "shared_files_tests.rs"]
mod tests;

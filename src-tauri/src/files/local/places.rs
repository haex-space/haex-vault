//! haex-vault's own places and the known places of the device (spec 017 FR-049, spec 044 FR-033,
//! FR-037, research R3).
//!
//! The own places are haex-vault's app folders (configuration, data, local data with the vaults, cache,
//! logs), resolved so a link cannot hide them. Extensions and agents never reach them; the user
//! sees them read-only. On Windows and macOS letter case does not count, as on their file systems.

use std::path::{Path, PathBuf};

use crate::files::local::resolve;

/// haex-vault's own app folders on this device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnPlaces {
    roots: Vec<PathBuf>,
}

impl OwnPlaces {
    /// Own places from already resolved roots (tests, and [`OwnPlaces::from_app`]).
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    /// The app folders of the running app, resolved.
    pub fn from_app<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Self {
        use tauri::Manager;
        let paths = app.path();
        let roots = [
            paths.app_config_dir(),
            paths.app_data_dir(),
            paths.app_local_data_dir(),
            paths.app_cache_dir(),
            paths.app_log_dir(),
        ]
        .into_iter()
        .filter_map(Result::ok)
        .map(|path| resolve(&path).unwrap_or(path))
        .collect();
        Self { roots }
    }

    /// Whether the resolved `path` lies in one of haex-vault's own places.
    pub fn contains(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| below(path, root))
    }

    /// Whether `path` is an own place, also when the path itself is a symbolic link into one.
    pub fn contains_resolved(&self, path: &Path) -> bool {
        self.contains(path)
            || std::fs::symlink_metadata(path).is_ok_and(|metadata| {
                metadata.file_type().is_symlink()
                    && std::fs::canonicalize(path).is_ok_and(|resolved| self.contains(&resolved))
            })
    }

    /// Whether a call on `path` touches an own place: `path` lies in one, or, for a call on the
    /// whole tree (`tree`), holds one.
    pub fn touches(&self, path: &Path, tree: bool) -> bool {
        self.roots
            .iter()
            .any(|root| below(path, root) || (tree && below(root, path)))
    }
}

/// Whether `path` is `root` or lies below it.
fn below(path: &Path, root: &Path) -> bool {
    #[cfg(any(windows, target_os = "macos"))]
    {
        let fold = |p: &Path| -> Vec<String> {
            p.components()
                .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
                .collect()
        };
        fold(path).starts_with(&fold(root))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        path.starts_with(root)
    }
}

/// A known place of the device (`home`, `documents`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub name: &'static str,
    pub path: PathBuf,
}

/// The known places of the running app that exist here, in a fixed order.
pub fn known_places<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<Place> {
    use tauri::Manager;
    let paths = app.path();
    #[allow(unused_mut)]
    let mut known = vec![
        ("home", paths.home_dir().ok()),
        ("pictures", paths.picture_dir().ok()),
        ("downloads", paths.download_dir().ok()),
        ("documents", paths.document_dir().ok()),
        ("videos", paths.video_dir().ok()),
    ];
    // Mobile systems have no desktop folder (spec 043 FR-016); same place in the list as before.
    #[cfg(desktop)]
    known.insert(4, ("desktop", paths.desktop_dir().ok()));
    existing_places(known)
}

/// The places among `candidates` whose folder exists.
pub fn existing_places(candidates: Vec<(&'static str, Option<PathBuf>)>) -> Vec<Place> {
    candidates
        .into_iter()
        .filter_map(|(name, path)| Some(Place { name, path: path? }))
        .filter(|place| place.path.exists())
        .collect()
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;

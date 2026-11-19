//! Chosen files as a desktop hands them over: plain paths (spec 043, contract `picked-file.md`).

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use haex_lib::files::picked::{Opener, PickedFile};
use tauri_plugin_fs::FilePath;

/// Opens paths with `std::fs`, as the file plugin does on a desktop.
#[derive(Clone, Copy)]
pub struct Paths;

impl Opener for Paths {
    fn open(&self, file: FilePath, write: bool) -> std::io::Result<std::fs::File> {
        let FilePath::Path(path) = file else {
            return Err(std::io::Error::other("paths only"));
        };
        if write {
            std::fs::File::create(path)
        } else {
            std::fs::File::open(path)
        }
    }

    fn path_roots(&self) -> Option<Vec<PathBuf>> {
        None
    }

    fn provider_name(&self, _uri: &str) -> Option<String> {
        None
    }
}

/// A path as the dialog hands it over.
pub fn chosen(path: &Path) -> PickedFile {
    PickedFile(path.to_string_lossy().into_owned())
}

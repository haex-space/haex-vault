//! What an import reads (spec 053 US3): a folder or an archive (a ZIP, also a VSIX, which is a ZIP
//! with `extension/`) seen as a list of regular files and a way to read one of them. Nothing is
//! written and nothing leaves the source: a link is never followed, a path that climbs out of an
//! archive refuses it, and what is read is bounded by what the caller asks for.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::Mutex;

use zip::ZipArchive;

use super::import::ImportError;

/// The largest archive that is read.
pub const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
/// The most entries of a source that are listed.
pub const MAX_ENTRIES: usize = 20_000;
/// Unpacked bytes of an archive per packed byte, at most (a ZIP bomb is far above it), and the
/// most it may declare altogether.
const RATIO: u64 = 200;
const MAX_DECLARED_BYTES: u64 = 512 * 1024 * 1024;

/// A source as a list of regular files by path with `/`, relative to its root.
pub trait Files {
    /// Every regular file with its size, sorted by path.
    fn files(&self) -> &BTreeMap<String, u64>;
    /// Whether `name` is a link (never read).
    fn is_link(&self, name: &str) -> bool;
    /// The bytes of `name`, at most `limit` of them.
    fn read(&self, name: &str, limit: u64) -> Result<Vec<u8>, ImportError>;
}

#[derive(Default)]
struct Listing {
    files: BTreeMap<String, u64>,
    links: BTreeSet<String>,
}

/// A folder of this device. Links are listed as links; `.git` and `node_modules` are not listed.
pub struct Folder {
    root: std::path::PathBuf,
    listing: Listing,
}

impl Folder {
    pub fn open(root: &Path) -> Result<Self, ImportError> {
        let mut listing = Listing::default();
        let walk = walkdir::WalkDir::new(root)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                !(entry.depth() > 0 && matches!(name.as_ref(), ".git" | "node_modules"))
            });
        for entry in walk {
            let entry = entry.map_err(|error| ImportError::Unreadable(error.to_string()))?;
            if entry.depth() == 0 {
                continue;
            }
            let Some(name) = entry
                .path()
                .strip_prefix(root)
                .ok()
                .and_then(|relative| {
                    relative
                        .components()
                        .map(|part| part.as_os_str().to_str())
                        .collect::<Option<Vec<_>>>()
                })
                .map(|parts| parts.join("/"))
            else {
                continue;
            };
            if listing.files.len() + listing.links.len() >= MAX_ENTRIES {
                return Err(ImportError::TooManyEntries);
            }
            if entry.path_is_symlink() {
                listing.links.insert(name);
            } else if entry.file_type().is_file() {
                let size = entry
                    .metadata()
                    .map_err(|error| ImportError::Unreadable(error.to_string()))?
                    .len();
                listing.files.insert(name, size);
            }
        }
        Ok(Self {
            root: root.to_owned(),
            listing,
        })
    }
}

impl Files for Folder {
    fn files(&self) -> &BTreeMap<String, u64> {
        &self.listing.files
    }

    fn is_link(&self, name: &str) -> bool {
        self.listing.links.contains(name)
    }

    fn read(&self, name: &str, limit: u64) -> Result<Vec<u8>, ImportError> {
        if !self.listing.files.contains_key(name) {
            return Err(ImportError::Unreadable(format!(
                "{name}: not in the folder"
            )));
        }
        let file = std::fs::File::open(self.root.join(name))
            .map_err(|error| ImportError::Unreadable(format!("{name}: {error}")))?;
        read_limited(file, name, limit)
    }
}

/// A ZIP held in memory.
pub struct Archive {
    archive: Mutex<ZipArchive<Cursor<Vec<u8>>>>,
    listing: Listing,
}

/// Whether a path of an archive could leave the place it is unpacked to: absolute, with a
/// backslash or NUL, with a `..` segment or with a drive letter.
fn climbs_out(name: &str) -> bool {
    name.starts_with('/')
        || name.contains(['\\', '\0'])
        || name
            .split('/')
            .any(|part| part == ".." || (part.len() == 2 && part.ends_with(':')))
}

impl Archive {
    pub fn open(bytes: Vec<u8>) -> Result<Self, ImportError> {
        if bytes.len() > MAX_ARCHIVE_BYTES {
            return Err(ImportError::ArchiveTooLarge);
        }
        let packed = bytes.len() as u64;
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| ImportError::Unreadable(error.to_string()))?;
        if archive.len() > MAX_ENTRIES {
            return Err(ImportError::TooManyEntries);
        }
        let mut listing = Listing::default();
        let mut declared = 0u64;
        for index in 0..archive.len() {
            let entry = archive
                .by_index_raw(index)
                .map_err(|error| ImportError::Unreadable(error.to_string()))?;
            let name = entry.name().to_owned();
            if climbs_out(&name) {
                return Err(ImportError::UnsafePath(name));
            }
            declared = declared.saturating_add(entry.size());
            if declared > MAX_DECLARED_BYTES || declared > packed.saturating_mul(RATIO).max(1 << 16)
            {
                return Err(ImportError::Ratio);
            }
            if entry.is_dir() {
                continue;
            }
            if entry.is_symlink() {
                listing.links.insert(name);
            } else if listing.files.insert(name.clone(), entry.size()).is_some() {
                return Err(ImportError::Unreadable(format!(
                    "{name}: twice in the archive"
                )));
            }
        }
        Ok(Self {
            archive: Mutex::new(archive),
            listing,
        })
    }
}

impl Files for Archive {
    fn files(&self) -> &BTreeMap<String, u64> {
        &self.listing.files
    }

    fn is_link(&self, name: &str) -> bool {
        self.listing.links.contains(name)
    }

    fn read(&self, name: &str, limit: u64) -> Result<Vec<u8>, ImportError> {
        let mut archive = self.archive.lock().unwrap_or_else(|e| e.into_inner());
        let entry = archive
            .by_name(name)
            .map_err(|error| ImportError::Unreadable(format!("{name}: {error}")))?;
        read_limited(entry, name, limit)
    }
}

fn read_limited(source: impl Read, name: &str, limit: u64) -> Result<Vec<u8>, ImportError> {
    let mut data = Vec::new();
    source
        .take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|error| ImportError::Unreadable(format!("{name}: {error}")))?;
    if data.len() as u64 > limit {
        return Err(ImportError::Unreadable(format!(
            "{name}: larger than it says"
        )));
    }
    Ok(data)
}

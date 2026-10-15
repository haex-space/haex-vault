//! The pack of a set (spec 053, data-model.md "Paket", research R6): a ZIP with the manifest
//! (`icon-theme.json`), the images and the license texts. The same code opens a bundled pack, a
//! custom set of the vault and a pack that came with the sync, so every pack passes the same
//! checks before one image of it is used (research R9, "empfangende Geräte").
//!
//! A pack is read in memory and never written to disk. It is small by its limits (5 MiB
//! unpacked), so opening it unpacks it whole once; an image is sanitized on its first use and the
//! result is kept ([`Pack::image`]).

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use haex_bundle::format::PathChecker;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use super::manifest::{IconTheme, ManifestError, MANIFEST_NAME, MAX_MANIFEST_BYTES};
use super::raster::{check_raster, RasterError};
use super::svg::{sanitize_svg, SvgError};
use super::ImageMime;
use crate::sync::keys::hex;

/// The limits of a pack (data-model.md); they hold for a bundled pack too.
pub mod limits {
    pub const IMAGES: usize = 2000;
    pub const IMAGE_BYTES: u64 = 256 * 1024;
    pub const UNPACKED_BYTES: u64 = 5 * 1024 * 1024;
    pub const PACKED_BYTES: usize = 8 * 1024 * 1024;
    /// Unpacked bytes per packed byte, at most (a ZIP bomb is far above it).
    pub const RATIO: u64 = 200;
    /// Entries that are not images: the manifest, the license texts.
    pub const OTHER_ENTRIES: usize = 16;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackError {
    #[error("the pack is larger than 8 MiB")]
    TooLarge,
    #[error("the archive cannot be read: {0}")]
    Archive(String),
    #[error("{0}")]
    Path(String),
    #[error("{0:?} is a directory, a link or no regular file")]
    EntryKind(String),
    #[error("{0:?} is larger than 256 KiB")]
    ImageTooLarge(String),
    #[error("the pack has more than 2000 images")]
    TooManyImages,
    #[error("the pack holds more than 5 MiB unpacked")]
    TooMuchData,
    #[error("the pack unpacks to more than 200 times its size")]
    Ratio,
    #[error("the pack has no icon-theme.json")]
    NoManifest,
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("the manifest names {0:?}, which is not in the pack")]
    MissingImage(String),
    #[error("the pack does not match its hash")]
    HashMismatch,
    #[error("no image {0:?} in the pack")]
    NoSuchImage(String),
    #[error("{path}: {reason}")]
    BadImage { path: String, reason: String },
}

/// An image as the window gets it: sanitized, with its media type.
#[derive(Debug)]
pub struct ServedImage {
    pub mime: ImageMime,
    pub bytes: Vec<u8>,
}

/// An opened pack: the manifest, the unpacked files and the images served so far.
#[derive(Debug)]
pub struct Pack {
    theme: IconTheme,
    files: BTreeMap<String, Vec<u8>>,
    image_count: u32,
    served: Mutex<HashMap<String, Arc<ServedImage>>>,
}

/// The SHA-256 of a pack as lower-case hex (the column `sha256`).
pub fn pack_hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn is_image(name: &str) -> bool {
    matches!(
        crate::files::kind::extension(name).as_str(),
        "svg" | "png" | "webp"
    )
}

/// Opens a pack and checks it: the size, every path (`haex_bundle::format`), the kind of every
/// entry, the limits, the manifest and that every image it names exists. With `expected_hash` the
/// bytes must hash to it. The checks of an image itself happen when it is served.
pub fn open_pack(bytes: &[u8], expected_hash: Option<&str>) -> Result<Pack, PackError> {
    if bytes.len() > limits::PACKED_BYTES {
        return Err(PackError::TooLarge);
    }
    if let Some(expected) = expected_hash {
        if pack_hash(bytes) != expected {
            return Err(PackError::HashMismatch);
        }
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(archive_error)?;
    let mut checker = PathChecker::default();
    let mut files = BTreeMap::new();
    let (mut images, mut others, mut total) = (0usize, 0usize, 0u64);
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(archive_error)?;
        let name = entry.name().to_owned();
        checker
            .check(&name)
            .map_err(|error| PackError::Path(error.to_string()))?;
        if !entry.is_file() || entry.is_symlink() {
            return Err(PackError::EntryKind(name));
        }
        let image = is_image(&name);
        let allowed = if image {
            limits::IMAGE_BYTES
        } else {
            MAX_MANIFEST_BYTES as u64
        };
        if image {
            images += 1;
            if images > limits::IMAGES {
                return Err(PackError::TooManyImages);
            }
        } else {
            others += 1;
            if others > limits::OTHER_ENTRIES {
                return Err(PackError::TooManyImages);
            }
        }
        if entry.size() > allowed {
            return Err(if image {
                PackError::ImageTooLarge(name)
            } else {
                PackError::Manifest(ManifestError::TooLarge)
            });
        }
        let mut data = Vec::new();
        entry
            .by_ref()
            .take(allowed + 1)
            .read_to_end(&mut data)
            .map_err(|error| PackError::Archive(error.to_string()))?;
        if data.len() as u64 > allowed {
            return Err(if image {
                PackError::ImageTooLarge(name)
            } else {
                PackError::Manifest(ManifestError::TooLarge)
            });
        }
        total += data.len() as u64;
        if total > limits::UNPACKED_BYTES {
            return Err(PackError::TooMuchData);
        }
        if total > (bytes.len() as u64).saturating_mul(limits::RATIO) {
            return Err(PackError::Ratio);
        }
        files.insert(name, data);
    }
    let manifest = files.get(MANIFEST_NAME).ok_or(PackError::NoManifest)?;
    let theme = IconTheme::parse(manifest)?;
    for path in theme.image_paths() {
        if !files.contains_key(path) {
            return Err(PackError::MissingImage(path.clone()));
        }
    }
    Ok(Pack {
        theme,
        files,
        image_count: images as u32,
        served: Mutex::new(HashMap::new()),
    })
}

fn archive_error(error: zip::result::ZipError) -> PackError {
    PackError::Archive(error.to_string())
}

impl Pack {
    pub fn theme(&self) -> &IconTheme {
        &self.theme
    }

    pub fn image_count(&self) -> u32 {
        self.image_count
    }

    /// The license texts of the pack, one after the other: `LICENSE` first, then every other
    /// `LICENSE*` file and `NOTICE`.
    pub fn license_text(&self) -> String {
        let mut names: Vec<&String> = self
            .files
            .keys()
            .filter(|name| name.starts_with("LICENSE") || name.as_str() == "NOTICE")
            .collect();
        names.sort_by_key(|name| {
            (
                name.as_str() != "LICENSE",
                name.as_str() == "NOTICE",
                name.to_string(),
            )
        });
        names
            .into_iter()
            .map(|name| String::from_utf8_lossy(&self.files[name]).trim().to_owned())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// The bytes of an image, ready for the window: an SVG sanitized, a PNG or WebP checked. The
    /// result is kept, so each image is made once per opened pack.
    pub fn image(&self, path: &str) -> Result<Arc<ServedImage>, PackError> {
        if let Some(served) = self
            .served
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
        {
            return Ok(Arc::clone(served));
        }
        let bytes = self
            .files
            .get(path)
            .filter(|_| is_image(path))
            .ok_or_else(|| PackError::NoSuchImage(path.to_owned()))?;
        let bad = |reason: String| PackError::BadImage {
            path: path.to_owned(),
            reason,
        };
        let served = if crate::files::kind::extension(path) == "svg" {
            ServedImage {
                mime: ImageMime::Svg,
                bytes: sanitize_svg(bytes).map_err(|error: SvgError| bad(error.to_string()))?,
            }
        } else {
            let kind =
                check_raster(path, bytes).map_err(|error: RasterError| bad(error.to_string()))?;
            ServedImage {
                mime: kind.mime(),
                bytes: bytes.clone(),
            }
        };
        let served = Arc::new(served);
        self.served
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.to_owned(), Arc::clone(&served));
        Ok(served)
    }

    /// The files of the pack by name, for the tests of the bundled packs.
    #[cfg(test)]
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
}

/// Packs a directory into a ZIP, the same way for every caller: entries sorted by path, fixed
/// timestamps, one compression, so the same directory gives the same bytes. Symbolic links and
/// anything that is not a file or a directory are refused.
pub fn pack_dir(dir: &Path) -> Result<Vec<u8>, PackError> {
    let mut names = Vec::new();
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(|error| PackError::Archive(error.to_string()))?;
        if entry.path_is_symlink() {
            return Err(PackError::EntryKind(entry.path().display().to_string()));
        }
        if entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() {
            return Err(PackError::EntryKind(entry.path().display().to_string()));
        }
        let relative = entry
            .path()
            .strip_prefix(dir)
            .map_err(|error| PackError::Archive(error.to_string()))?;
        let name = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        names.push((name, entry.into_path()));
    }
    names.sort();
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, path) in names {
        let data = std::fs::read(&path).map_err(|error| PackError::Archive(error.to_string()))?;
        writer.start_file(name, options).map_err(archive_error)?;
        writer
            .write_all(&data)
            .map_err(|error| PackError::Archive(error.to_string()))?;
    }
    Ok(writer.finish().map_err(archive_error)?.into_inner())
}

#[cfg(test)]
#[path = "pack_tests.rs"]
mod tests;

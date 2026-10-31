//! Thumbnails for list and grid (spec 044 FR-005, research R7): decoded in Rust with limits,
//! turned upright by their EXIF orientation, shrunk to [`EDGE`] and cached as JPEG in holzi's cache
//! (never synced, ADR 0001; an own place, so agents never reach it). The key covers source, path,
//! size and modification time, so a changed file gets a new thumbnail. An image that fails leaves a
//! marker and is not tried again until it changes.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, PixelType, ResizeOptions, Resizer};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader, Limits, RgbImage, RgbaImage};
use sha2::{Digest, Sha256};

use crate::files::{FilesError, FilesErrorCode};

/// The longest edge of a thumbnail.
pub const EDGE: u32 = 320;

/// JPEG quality of a thumbnail.
const QUALITY: u8 = 80;

/// Largest width or height a thumbnail is made from.
const MAX_SIDE: u32 = 20_000;

/// Most memory one decode may take.
const MAX_ALLOC: u64 = 512 * 1024 * 1024;

/// The folder below holzi's cache directory.
pub const CACHE_FOLDER: &str = "files-thumbnails";

/// The cache key of a thumbnail: hex SHA-256 over source, path, size and modification time.
pub fn cache_key(source: &str, path: &str, size: u64, modified_ms: i64) -> String {
    let mut hasher = Sha256::new();
    for part in [
        source.as_bytes(),
        path.as_bytes(),
        &size.to_le_bytes(),
        &modified_ms.to_le_bytes(),
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The JPEG thumbnail of the image at `path`, from the cache in `cache_dir` or made now.
pub fn thumbnail(
    cache_dir: &Path,
    source: &str,
    path: &Path,
    size: u64,
    modified_ms: i64,
) -> Result<Vec<u8>, FilesError> {
    let name = path.to_string_lossy();
    cached(cache_dir, source, &name, size, modified_ms)
        .unwrap_or_else(|| render_into_cache(cache_dir, source, &name, size, modified_ms, path))
}

/// The thumbnail of `name` in `source` from the cache: `None` when it was not made yet, an error
/// when the image could not be decoded before (a storage asks before it downloads, spec 044 US5).
pub fn cached(
    cache_dir: &Path,
    source: &str,
    name: &str,
    size: u64,
    modified_ms: i64,
) -> Option<Result<Vec<u8>, FilesError>> {
    let key = cache_key(source, name, size, modified_ms);
    if let Ok(bytes) = std::fs::read(cache_dir.join(format!("{key}.jpg"))) {
        return Some(Ok(bytes));
    }
    cache_dir
        .join(format!("{key}.broken"))
        .exists()
        .then(|| Err(broken_error()))
}

/// Makes the thumbnail of `name` in `source` from the image in `file` and caches it (or marks it
/// broken).
pub fn render_into_cache(
    cache_dir: &Path,
    source: &str,
    name: &str,
    size: u64,
    modified_ms: i64,
    file: &Path,
) -> Result<Vec<u8>, FilesError> {
    let key = cache_key(source, name, size, modified_ms);
    let cached = cache_dir.join(format!("{key}.jpg"));
    let broken = cache_dir.join(format!("{key}.broken"));
    match render(file) {
        Ok(bytes) => {
            if std::fs::create_dir_all(cache_dir).is_ok() {
                if let Err(error) = write_atomically(&cached, &bytes) {
                    log::warn!("files: could not cache a thumbnail: {error}");
                }
            }
            Ok(bytes)
        }
        Err(reason) => {
            log::info!(
                "files: no thumbnail for {}: {reason}",
                crate::files::encrypted::redacted::Redacted(name)
            );
            if std::fs::create_dir_all(cache_dir).is_ok() {
                let _ = std::fs::write(&broken, b"");
            }
            Err(broken_error())
        }
    }
}

fn broken_error() -> FilesError {
    FilesError::new(FilesErrorCode::Broken, "no thumbnail for this image")
}

fn write_atomically(target: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    let part = target.with_extension("jpg.part");
    std::fs::write(&part, bytes)?;
    std::fs::rename(part, target)
}

fn render(path: &Path) -> Result<Vec<u8>, String> {
    let reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    render_from(reader)
}

/// The thumbnail of the image in `bytes`, made in memory (spec 048: entries of encrypted folders
/// never touch the disk).
pub fn render_bytes(bytes: &[u8]) -> Result<Vec<u8>, FilesError> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())
        .and_then(render_from)
        .map_err(|_| broken_error())
}

fn render_from<R: std::io::BufRead + std::io::Seek>(
    reader: ImageReader<R>,
) -> Result<Vec<u8>, String> {
    let small = upright_within(reader, EDGE)?;
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut Cursor::new(&mut bytes), QUALITY)
        .encode_image(&small)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// The image of `reader` decoded within the limits, turned upright by its EXIF orientation and
/// shrunk so its longest edge is at most `edge` (never enlarged). Thumbnails and the images agents
/// read (`files::extract::image`) share it.
pub(crate) fn upright_within<R: std::io::BufRead + std::io::Seek>(
    reader: ImageReader<R>,
    edge: u32,
) -> Result<RgbImage, String> {
    let image = decode_upright(reader, MAX_SIDE, MAX_ALLOC)?;
    let rgb = DynamicImage::ImageRgb8(image.to_rgb8());
    let (width, height, pixels) = shrink(&rgb, edge, PixelType::U8x3)?;
    RgbImage::from_raw(width, height, pixels)
        .ok_or_else(|| "resized buffer has the wrong size".to_owned())
}

/// Like [`upright_within`], but keeping the transparency and with the decode limits given: the
/// icons of a marketplace (spec 069 R12).
pub(crate) fn upright_rgba_within<R: std::io::BufRead + std::io::Seek>(
    reader: ImageReader<R>,
    edge: u32,
    max_side: u32,
    max_alloc: u64,
) -> Result<RgbaImage, String> {
    let image = decode_upright(reader, max_side, max_alloc)?;
    let rgba = DynamicImage::ImageRgba8(image.to_rgba8());
    let (width, height, pixels) = shrink(&rgba, edge, PixelType::U8x4)?;
    RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| "resized buffer has the wrong size".to_owned())
}

/// The image of `reader`, decoded with at most `max_side` per side and `max_alloc` bytes, and
/// turned upright by its EXIF orientation.
fn decode_upright<R: std::io::BufRead + std::io::Seek>(
    mut reader: ImageReader<R>,
    max_side: u32,
    max_alloc: u64,
) -> Result<DynamicImage, String> {
    let mut limits = Limits::default();
    limits.max_image_width = Some(max_side);
    limits.max_image_height = Some(max_side);
    limits.max_alloc = Some(max_alloc);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    Ok(image)
}

/// The pixels of `image` (of the kind `pixel`) shrunk so the longest edge is at most `edge`, never
/// enlarged, with the new width and height. Transparent pixels do not darken their neighbours.
fn shrink(
    image: &DynamicImage,
    edge: u32,
    pixel: PixelType,
) -> Result<(u32, u32, Vec<u8>), String> {
    if image.pixel_type() != Some(pixel) {
        return Err("unexpected pixel type".to_owned());
    }
    let (width, height) = fit(image.width(), image.height(), edge);
    if (width, height) == (image.width(), image.height()) {
        return Ok((width, height, image.as_bytes().to_vec()));
    }
    let mut target = Image::new(width, height, pixel);
    Resizer::new()
        .resize(image, &mut target, &ResizeOptions::new())
        .map_err(|e| e.to_string())?;
    Ok((width, height, target.into_vec()))
}

/// The most bytes [`MemoryThumbnails`] keeps.
pub const MEMORY_LIMIT: usize = 64 * 1024 * 1024;

/// Thumbnails of entries in encrypted folders (spec 048 FR-026, research R10): only in memory, at
/// most [`MEMORY_LIMIT`] bytes, the least recently used go first; emptied when the vault closes.
/// `None` marks an image that could not be decoded.
#[derive(Default)]
pub struct MemoryThumbnails {
    kept: std::sync::Mutex<Kept>,
}

// ponytail: a linear scan over a queue; at ~20 KB per thumbnail that is a few thousand entries. A
// map with an order list is the upgrade if it ever shows in a profile.
#[derive(Default)]
struct Kept {
    /// Least recently used first.
    order: std::collections::VecDeque<(String, Option<Vec<u8>>)>,
    bytes: usize,
}

impl MemoryThumbnails {
    fn kept(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.kept
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The thumbnail under `key`, if one was made in this session.
    pub fn get(&self, key: &str) -> Option<Result<Vec<u8>, FilesError>> {
        let mut kept = self.kept();
        let at = kept.order.iter().position(|(k, _)| k == key)?;
        let hit = kept.order.remove(at)?;
        let answer = hit.1.clone().ok_or_else(broken_error);
        kept.order.push_back(hit);
        Some(answer)
    }

    /// Keeps `made` under `key`, dropping the least recently used beyond the limit.
    pub fn put(&self, key: String, made: &Result<Vec<u8>, FilesError>) {
        let bytes = made.as_ref().ok().cloned();
        let size = bytes.as_ref().map_or(0, Vec::len);
        if size > MEMORY_LIMIT {
            return;
        }
        let mut kept = self.kept();
        if let Some(at) = kept.order.iter().position(|(k, _)| *k == key) {
            if let Some((_, old)) = kept.order.remove(at) {
                kept.bytes -= old.map_or(0, |old| old.len());
            }
        }
        kept.order.push_back((key, bytes));
        kept.bytes += size;
        while kept.bytes > MEMORY_LIMIT {
            let Some((_, old)) = kept.order.pop_front() else {
                break;
            };
            kept.bytes -= old.map_or(0, |old| old.len());
        }
    }

    /// Forgets every thumbnail (the vault closes).
    pub fn clear(&self) {
        let mut kept = self.kept();
        kept.order.clear();
        kept.bytes = 0;
    }

    /// The bytes kept.
    pub fn size(&self) -> usize {
        self.kept().bytes
    }
}

/// The size within `edge` × `edge` keeping the shape; never larger than the original.
fn fit(width: u32, height: u32, edge: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= edge || longest == 0 {
        return (width, height);
    }
    let scale =
        |side: u32| ((u64::from(side) * u64::from(edge)) / u64::from(longest)).max(1) as u32;
    (scale(width), scale(height))
}

#[cfg(test)]
#[path = "thumbnails_tests.rs"]
pub(crate) mod tests;

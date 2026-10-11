use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageFormat, RgbImage};

use super::*;
use crate::files::FilesErrorCode;

fn write_image(path: &Path, width: u32, height: u32, format: ImageFormat) {
    let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(
        width,
        height,
        image::Rgb([200, 40, 40]),
    ));
    image.save_with_format(path, format).unwrap();
}

fn dims(jpeg: &[u8]) -> (u32, u32) {
    let image = image::load_from_memory_with_format(jpeg, ImageFormat::Jpeg).unwrap();
    (image.width(), image.height())
}

/// A JPEG of `width` × `height` with an EXIF orientation tag (APP1 right after SOI).
fn jpeg_with_orientation(width: u32, height: u32, orientation: u16) -> Vec<u8> {
    let mut plain = Vec::new();
    DynamicImage::ImageRgb8(RgbImage::new(width, height))
        .write_to(&mut Cursor::new(&mut plain), ImageFormat::Jpeg)
        .unwrap();
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II*\0");
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&orientation.to_le_bytes());
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&0u32.to_le_bytes());
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(&tiff);
    let length = u16::try_from(payload.len() + 2).unwrap();
    let mut out = plain[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&plain[2..]);
    out
}

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("cache");
    let files = dir.path().join("files");
    std::fs::create_dir_all(&files).unwrap();
    (dir, cache, files)
}

#[test]
fn the_cache_key_changes_with_size_and_time() {
    let base = cache_key("device", "/a.png", 10, 1000);
    assert_eq!(base.len(), 64);
    assert_ne!(base, cache_key("device", "/a.png", 11, 1000));
    assert_ne!(base, cache_key("device", "/a.png", 10, 1001));
    assert_ne!(base, cache_key("storage:s1", "/a.png", 10, 1000));
    assert_eq!(base, cache_key("device", "/a.png", 10, 1000));
}

#[test]
fn a_large_image_shrinks_to_the_edge_keeping_its_shape() {
    let (_dir, cache, files) = setup();
    let path = files.join("breit.png");
    write_image(&path, 640, 320, ImageFormat::Png);
    let bytes = thumbnail(&cache, "device", &path, 1, 1).unwrap();
    assert_eq!(dims(&bytes), (EDGE, EDGE / 2));
}

#[test]
fn a_small_image_is_not_enlarged() {
    let (_dir, cache, files) = setup();
    let path = files.join("klein.png");
    write_image(&path, 40, 30, ImageFormat::Png);
    assert_eq!(
        dims(&thumbnail(&cache, "device", &path, 1, 1).unwrap()),
        (40, 30)
    );
}

#[test]
fn exif_orientation_turns_the_thumbnail_upright() {
    let (_dir, cache, files) = setup();
    let path = files.join("handy.jpg");
    std::fs::write(&path, jpeg_with_orientation(40, 20, 6)).unwrap();
    assert_eq!(
        dims(&thumbnail(&cache, "device", &path, 1, 1).unwrap()),
        (20, 40)
    );
}

#[test]
fn a_second_request_comes_from_the_cache() {
    let (_dir, cache, files) = setup();
    let path = files.join("a.png");
    write_image(&path, 64, 64, ImageFormat::Png);
    let first = thumbnail(&cache, "device", &path, 5, 7).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(thumbnail(&cache, "device", &path, 5, 7).unwrap(), first);
}

#[test]
fn a_broken_image_is_marked_and_not_tried_again() {
    let (_dir, cache, files) = setup();
    let path = files.join("kaputt.jpg");
    std::fs::write(&path, b"not an image").unwrap();
    assert_eq!(
        thumbnail(&cache, "device", &path, 3, 3).unwrap_err().code,
        FilesErrorCode::Broken
    );
    // Same size and time: the marker answers, even though the file is fine now.
    write_image(&path, 10, 10, ImageFormat::Png);
    assert_eq!(
        thumbnail(&cache, "device", &path, 3, 3).unwrap_err().code,
        FilesErrorCode::Broken
    );
    // A changed file has another key and is tried again.
    assert!(thumbnail(&cache, "device", &path, 4, 4).is_ok());
}

/// A PNG that only declares huge dimensions, with a valid header.
fn huge_png() -> Vec<u8> {
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in bytes {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    let mut ihdr = b"IHDR".to_vec();
    ihdr.extend_from_slice(&30_000u32.to_be_bytes());
    ihdr.extend_from_slice(&30_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(&ihdr);
    png.extend_from_slice(&crc32(&ihdr).to_be_bytes());
    png
}

#[test]
fn an_image_beyond_the_limits_is_refused_without_decoding() {
    let (_dir, cache, files) = setup();
    let path = files.join("riesig.png");
    std::fs::write(&path, huge_png()).unwrap();
    assert_eq!(
        thumbnail(&cache, "device", &path, 1, 1).unwrap_err().code,
        FilesErrorCode::Broken
    );
}

#[test]
fn memory_thumbnails_keep_the_most_recent_within_the_limit() {
    let kept = MemoryThumbnails::default();
    let big = vec![0u8; MEMORY_LIMIT / 2];
    kept.put("a".to_owned(), &Ok(big.clone()));
    kept.put("b".to_owned(), &Ok(big.clone()));
    assert!(kept.get("a").is_some(), "a is now the most recent");
    kept.put("c".to_owned(), &Ok(big));
    assert!(kept.get("b").is_none(), "the least recent went");
    assert!(kept.get("a").is_some());
    assert!(kept.size() <= MEMORY_LIMIT);
    kept.put("kaputt".to_owned(), &Err(broken_error()));
    assert_eq!(
        kept.get("kaputt").unwrap().unwrap_err().code,
        FilesErrorCode::Broken
    );
    kept.clear();
    assert_eq!(kept.size(), 0);
    assert!(kept.get("a").is_none());
}

#[test]
fn bytes_that_are_no_image_are_broken() {
    assert_eq!(
        render_bytes(b"kein Bild").unwrap_err().code,
        FilesErrorCode::Broken
    );
}

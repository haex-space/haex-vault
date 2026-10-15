//! Helpers of the import tests (T041): the fixture folders and the archives and images the tests
//! build, so no binary file is checked in.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/icon_sets")
        .join(name)
}

/// A ZIP of the given files in the given order, with the given names as they are.
pub fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .expect("start");
        writer.write_all(data).expect("write");
    }
    writer.finish().expect("finish").into_inner()
}

/// A ZIP of every file of a directory, under `prefix`.
pub fn zip_of_dir(dir: &Path, prefix: &str) -> Vec<u8> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.expect("walk");
        if entry.file_type().is_file() {
            let relative = entry.path().strip_prefix(dir).expect("inside");
            let name = format!("{prefix}{}", relative.to_string_lossy().replace('\\', "/"));
            files.push((name, std::fs::read(entry.path()).expect("read")));
        }
    }
    let borrowed: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(name, data)| (name.as_str(), data.as_slice()))
        .collect();
    zip_of(&borrowed)
}

/// A PNG of the size with pixels that do not compress (the same bytes for the same seed).
pub fn png(width: u32, height: u32, seed: u64) -> Vec<u8> {
    let mut state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let image = image::RgbImage::from_fn(width, height, |_, _| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        image::Rgb([
            (state >> 33) as u8,
            (state >> 41) as u8,
            (state >> 49) as u8,
        ])
    });
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("png");
    out.into_inner()
}

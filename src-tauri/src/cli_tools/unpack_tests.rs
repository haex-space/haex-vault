//! Unpacking and placing a downloaded tool (spec 094, T014, FR-005, FR-007).

use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;
use tar::{Builder, EntryType, Header};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::{place_file, unpack_tar_gz, BoundedReader, UnpackError};

/// A header with the path written byte for byte, so a test can name a path the `tar` crate's own
/// builder would refuse.
fn raw_header(path: &str, kind: EntryType, size: u64) -> Header {
    let mut header = Header::new_gnu();
    header.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
    header.set_entry_type(kind);
    header.set_size(size);
    header.set_mode(0o644);
    header.set_cksum();
    header
}

fn archive_of(dir: &Path, build: impl FnOnce(&mut Builder<GzEncoder<File>>)) -> std::path::PathBuf {
    let path = dir.join("package.tar.gz");
    let encoder = GzEncoder::new(File::create(&path).unwrap(), Compression::fast());
    let mut builder = Builder::new(encoder);
    build(&mut builder);
    builder
        .into_inner()
        .unwrap()
        .finish()
        .unwrap()
        .flush()
        .unwrap();
    path
}

fn add_file(builder: &mut Builder<GzEncoder<File>>, path: &str, mode: u32, data: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(mode);
    header.set_cksum();
    builder.append_data(&mut header, path, data).unwrap();
}

fn unpack(archive: &Path, target: &Path, limit: u64) -> Result<u64, UnpackError> {
    unpack_tar_gz(archive, target, limit, &CancellationToken::new())
}

fn target(dir: &TempDir) -> std::path::PathBuf {
    let target = dir.path().join("staging");
    fs::create_dir(&target).unwrap();
    target
}

#[test]
fn a_package_is_unpacked_with_its_folders_files_and_sizes() {
    let dir = TempDir::new().unwrap();
    let archive = archive_of(dir.path(), |builder| {
        add_file(builder, "codex-package.json", 0o644, b"{\"v\":1}");
        add_file(builder, "bin/codex", 0o755, &[7u8; 100]);
        add_file(builder, "codex-resources/data.bin", 0o644, &[1u8; 50]);
    });
    let target = target(&dir);

    let written = unpack(&archive, &target, 157).unwrap();

    assert_eq!(written, 7 + 100 + 50);
    assert_eq!(
        fs::read(target.join("codex-package.json")).unwrap(),
        b"{\"v\":1}"
    );
    assert_eq!(fs::metadata(target.join("bin/codex")).unwrap().len(), 100);
    assert_eq!(
        fs::metadata(target.join("codex-resources/data.bin"))
            .unwrap()
            .len(),
        50
    );
}

#[cfg(unix)]
#[test]
fn the_executable_bit_of_the_package_is_kept() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let archive = archive_of(dir.path(), |builder| {
        add_file(builder, "bin/codex", 0o755, b"x");
        add_file(builder, "plain.txt", 0o644, b"x");
    });
    let target = target(&dir);
    unpack(&archive, &target, 2).unwrap();
    let mode = |p: &str| fs::metadata(target.join(p)).unwrap().permissions().mode() & 0o111;
    assert_ne!(mode("bin/codex"), 0);
    assert_eq!(mode("plain.txt"), 0);
}

#[test]
fn an_entry_that_leaves_the_target_is_refused_and_nothing_is_written_outside() {
    for path in ["../escape.txt", "a/../../escape.txt", "/etc/escape.txt"] {
        let dir = TempDir::new().unwrap();
        let archive = archive_of(dir.path(), |builder| {
            builder
                .append(&raw_header(path, EntryType::Regular, 4), &b"evil"[..])
                .unwrap();
        });
        let target = target(&dir);
        let result = unpack(&archive, &target, 100);
        assert!(
            matches!(result, Err(UnpackError::Refused(_))),
            "{path}: {result:?}"
        );
        assert!(!dir.path().join("escape.txt").exists(), "{path}");
        assert!(!Path::new("/etc/escape.txt").exists());
    }
}

#[test]
fn links_and_devices_are_refused() {
    for kind in [
        EntryType::Symlink,
        EntryType::Link,
        EntryType::Char,
        EntryType::Block,
        EntryType::Fifo,
    ] {
        let dir = TempDir::new().unwrap();
        let archive = archive_of(dir.path(), |builder| {
            add_file(builder, "ok.txt", 0o644, b"ok");
            let mut header = raw_header("thing", kind, 0);
            header.set_link_name("/etc/passwd").unwrap();
            header.set_cksum();
            builder.append(&header, std::io::empty()).unwrap();
        });
        let target = target(&dir);
        let result = unpack(&archive, &target, 100);
        assert!(
            matches!(result, Err(UnpackError::Refused(_))),
            "{kind:?}: {result:?}"
        );
        assert!(!target.join("thing").exists(), "{kind:?}");
    }
}

#[test]
fn an_archive_larger_than_the_catalog_lists_is_refused() {
    let dir = TempDir::new().unwrap();
    let archive = archive_of(dir.path(), |builder| {
        add_file(builder, "big.bin", 0o644, &vec![0u8; 3 * 1024 * 1024]);
    });
    let target = target(&dir);
    // The catalog says 100 bytes; the slack is 1 MiB, the entry is 3 MiB.
    let result = unpack(&archive, &target, 100);
    assert!(matches!(result, Err(UnpackError::Refused(_))), "{result:?}");
    assert!(!target.join("big.bin").exists());
}

#[test]
fn a_cut_off_or_foreign_archive_is_an_io_error_not_a_partial_success() {
    let dir = TempDir::new().unwrap();
    let archive = archive_of(dir.path(), |builder| {
        add_file(builder, "bin/codex", 0o755, &[9u8; 4000]);
    });
    let bytes = fs::read(&archive).unwrap();
    let cut = dir.path().join("cut.tar.gz");
    fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
    let target_dir = target(&dir);
    assert!(matches!(
        unpack(&cut, &target_dir, 10_000),
        Err(UnpackError::Io(_))
    ));

    let not_gzip = dir.path().join("plain.tar.gz");
    fs::write(&not_gzip, b"this is not a gzip stream at all").unwrap();
    assert!(unpack(&not_gzip, &target_dir, 10_000).is_err());
    assert!(matches!(
        unpack(&dir.path().join("missing.tar.gz"), &target_dir, 10),
        Err(UnpackError::Io(_))
    ));
}

#[test]
fn a_cancelled_unpack_stops() {
    let dir = TempDir::new().unwrap();
    let archive = archive_of(dir.path(), |builder| {
        add_file(builder, "a", 0o644, b"a");
    });
    let target = target(&dir);
    let token = CancellationToken::new();
    token.cancel();
    assert_eq!(
        unpack_tar_gz(&archive, &target, 10, &token),
        Err(UnpackError::Cancelled)
    );
    assert!(!target.join("a").exists());
}

#[test]
fn decompressed_archive_input_has_a_hard_limit() {
    let mut reader = BoundedReader {
        inner: Cursor::new(b"oversized"),
        remaining: 4,
        cancel: CancellationToken::new(),
    };
    let mut output = Vec::new();
    assert!(reader.read_to_end(&mut output).is_err());
    assert_eq!(output, b"over");
}

#[test]
fn a_raw_program_is_moved_to_its_name_and_made_executable() {
    let dir = TempDir::new().unwrap();
    let downloaded = dir.path().join("download");
    fs::write(&downloaded, b"#!/bin/sh\n").unwrap();
    let target = dir.path().join("staging").join("bin").join("claude");

    place_file(&downloaded, &target).unwrap();

    assert!(!downloaded.exists());
    assert_eq!(fs::read(&target).unwrap(), b"#!/bin/sh\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o111,
            0
        );
    }
    // A program already downloaded under its own name is only made executable.
    place_file(&target, &target).unwrap();
    assert!(target.is_file());
}

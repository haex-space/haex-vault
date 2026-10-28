//! The building blocks of [`super::SftpStore`] (spec 050 research R3, R7–R9): paths below the
//! folder of a storage, one level of a folder as holzi lists it, missing folders made below the
//! storage's folder (never the storage's folder itself, FR-006), a file written through a
//! temporary file, and a folder removed from the bottom up.

use russh_sftp::protocol::FileAttributes;
use tokio::io::AsyncWriteExt;

use super::super::tree::{ancestors, parent_of, part_name, DirEntry};
use super::super::{Location, StorageError, StorageSpace};
use super::connect::{sftp_error, Session};
use super::rename::rename_over;
use crate::passwords::clock::format_millis;

/// The path of `key` below the folder of `location`, relative to the login's home folder; a
/// folder key (ending in `/`) or an empty key gives the folder without `/`, the home is `.`. A
/// part `.` or `..` would lead out of the folder and reaches nothing.
pub(super) fn path_of(location: &Location, key: &str) -> Result<String, StorageError> {
    let dots = |part: &str| part == "." || part == "..";
    if location.bucket.split('/').any(dots) || key.split('/').any(dots) {
        log::warn!("remote storage: an SFTP key with `.` or `..` was refused");
        return Err(StorageError::NotFound);
    }
    let parts: Vec<&str> = location
        .bucket
        .split('/')
        .chain(key.split('/'))
        .filter(|part| !part.is_empty())
        .collect();
    Ok(if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    })
}

/// The space of a file system from its `statvfs` numbers (`blocks` of `fragment_size` bytes, of
/// which `blocks_avail` are free for this login); `None` for a file system that reports no size.
pub(super) fn space_of(fragment_size: u64, blocks: u64, blocks_avail: u64) -> Option<StorageSpace> {
    let total = blocks
        .checked_mul(fragment_size)
        .filter(|total| *total > 0)?;
    let free = blocks_avail.checked_mul(fragment_size)?.min(total);
    Some(StorageSpace {
        used: total - free,
        total: Some(total),
    })
}

/// The time and the mark of a file as an S3 listing gives them: ISO time, `"<size>-<mtime>"`.
pub(super) fn stamp(attrs: &FileAttributes) -> (String, String) {
    let mtime = attrs.mtime.unwrap_or(0);
    let modified = if attrs.mtime.is_some() {
        format_millis(i64::from(mtime) * 1000)
    } else {
        String::new()
    };
    (modified, format!("{}-{mtime}", attrs.size.unwrap_or(0)))
}

/// The entries of the folder `dir` (a key ending in `/`, or empty), without `.` and `..`.
pub(super) async fn read_dir(
    session: &Session,
    location: &Location,
    dir: &str,
) -> Result<Vec<DirEntry>, StorageError> {
    let path = path_of(location, dir)?;
    let entries = session.sftp.read_dir(path).await.map_err(sftp_error)?;
    Ok(entries
        .filter(|entry| {
            let name = entry.file_name();
            name != "." && name != ".." && !name.contains('/')
        })
        .map(|entry| {
            let attrs = entry.metadata();
            let (modified, etag) = stamp(&attrs);
            DirEntry {
                name: entry.file_name(),
                is_dir: attrs.is_dir(),
                size: if attrs.is_dir() { 0 } else { attrs.len() },
                modified,
                etag,
            }
        })
        .collect())
}

/// Makes the folder `path` unless it is there.
async fn make_folder(session: &Session, path: &str) -> Result<(), StorageError> {
    match session.sftp.metadata(path).await.map_err(sftp_error) {
        Ok(attrs) if attrs.is_dir() => Ok(()),
        Ok(_) => Err(StorageError::NotFound),
        Err(StorageError::NotFound) => session.sftp.create_dir(path).await.map_err(sftp_error),
        Err(error) => Err(error),
    }
}

/// Makes the folders of `key` below the folder of the storage, from the top. The storage's folder
/// must be there: a missing one is [`StorageError::NotFound`] (FR-006).
pub(super) async fn make_parents(
    session: &Session,
    location: &Location,
    key: &str,
) -> Result<(), StorageError> {
    let root = path_of(location, "")?;
    match session.sftp.metadata(root).await.map_err(sftp_error)? {
        attrs if attrs.is_dir() => {}
        _ => return Err(StorageError::NotFound),
    }
    for folder in ancestors(key) {
        make_folder(session, &path_of(location, folder)?).await?;
    }
    Ok(())
}

/// Makes the folder `key` (ending in `/`) with its missing parents below the storage's folder.
pub(super) async fn put_folder(
    session: &Session,
    location: &Location,
    key: &str,
) -> Result<(), StorageError> {
    make_parents(session, location, key).await?;
    make_folder(session, &path_of(location, key)?).await
}

/// Writes `body` to a temporary file next to `key` and renames it onto `key`; a failure removes
/// the temporary file.
pub(super) async fn put_file(
    session: &Session,
    location: &Location,
    key: &str,
    body: &[u8],
) -> Result<(), StorageError> {
    make_parents(session, location, key).await?;
    let part = path_of(location, &format!("{}{}", parent_of(key), part_name()))?;
    let written = async {
        let mut file = session
            .sftp
            .create(part.clone())
            .await
            .map_err(sftp_error)?;
        file.write_all(body)
            .await
            .map_err(|_| StorageError::Network)?;
        file.shutdown().await.map_err(|_| StorageError::Network)?;
        rename_over(session, &part, &path_of(location, key)?).await
    }
    .await;
    if written.is_err() {
        let _ = session.sftp.remove_file(part).await;
    }
    written
}

/// Removes the folder `path` with everything in it, from the bottom up.
pub(super) async fn remove_tree(session: &Session, path: &str) -> Result<(), StorageError> {
    let mut folders = vec![path.to_owned()];
    let mut index = 0;
    while index < folders.len() {
        let folder = folders[index].clone();
        let entries = match session
            .sftp
            .read_dir(folder.clone())
            .await
            .map_err(sftp_error)
        {
            Ok(entries) => entries,
            Err(StorageError::NotFound) if index == 0 => return Ok(()),
            Err(error) => return Err(error),
        };
        for entry in entries {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child = format!("{folder}/{name}");
            if entry.metadata().is_dir() {
                folders.push(child);
            } else {
                session.sftp.remove_file(child).await.map_err(sftp_error)?;
            }
        }
        index += 1;
    }
    for folder in folders.iter().rev() {
        session
            .sftp
            .remove_dir(folder.clone())
            .await
            .map_err(sftp_error)?;
    }
    Ok(())
}

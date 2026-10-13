//! A folder on this device behind the one storage seam (spec 048 US7, research R15): a key is a
//! path below the root in `Location.bucket`, folders are real folders as on WebDAV and SFTP, and a
//! file is written to a temporary file next to its name and moved onto it, so nothing half stays
//! under the name (FR-047). It is chosen as [`Protocol::Local`], which no storage is ever saved
//! with; encrypted folders on the device are its only user.

use std::collections::HashMap;
use std::fs;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::time::Instant;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::local_fs::{
    append, error_of, etag_of, http_date_of, make_parent, part_of, read_entries, resolve,
    resolve_below, settle, write_whole,
};
use super::tree;
use super::{
    Access, Addressing, Credentials, DirListing, EndpointScope, Location, ObjectHead, ObjectInfo,
    PasswordCredentials, Protocol, ProviderKind, RemoteStore, StorageError,
};

/// What a call needs to reach the folder `root` (absolute) on this device; it needs no
/// credentials, so they are empty.
pub fn access_of(root: &Path) -> Access {
    Access {
        location: Location {
            protocol: Protocol::Local,
            provider_kind: ProviderKind::Other,
            endpoint: String::new(),
            endpoint_scope: EndpointScope::Local,
            region: String::new(),
            addressing: Addressing::Path,
            bucket: root.to_string_lossy().into_owned(),
            host_key: None,
        },
        credentials: Credentials::Password(PasswordCredentials {
            username: String::new(),
            password: Zeroizing::new(String::new()),
        }),
    }
}

/// The local implementation of [`RemoteStore`].
#[derive(Default)]
pub struct LocalStore {
    /// The temporary file of each running upload, by its id.
    uploads: Mutex<HashMap<String, PathBuf>>,
}

impl LocalStore {
    /// A store without running uploads.
    pub fn new() -> Self {
        Self::default()
    }
}

/// The root of `access`.
fn root(access: &Access) -> PathBuf {
    PathBuf::from(&access.location.bucket)
}

/// Runs the file system work `work` off the async threads, until `deadline`.
async fn blocking<T, F>(deadline: Instant, work: F) -> Result<T, StorageError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, StorageError> + Send + 'static,
{
    tokio::time::timeout_at(deadline, tokio::task::spawn_blocking(work))
        .await
        .map_err(|_| StorageError::TimedOut)?
        .map_err(|_| StorageError::Network)?
}

#[async_trait]
impl RemoteStore for LocalStore {
    /// A file through a temporary file; a key ending in `/` creates the folder.
    async fn put(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let path = resolve_below(&root(access), key)?;
        let folder = key.ends_with('/');
        blocking(deadline, move || {
            if folder {
                fs::create_dir_all(&path).map_err(error_of)
            } else {
                write_whole(&path, &body)
            }
        })
        .await
    }

    async fn get(
        &self,
        access: &Access,
        key: &str,
        max_bytes: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>, StorageError> {
        let path = resolve(&root(access), key)?;
        blocking(deadline, move || {
            let meta = fs::metadata(&path).map_err(error_of)?;
            if usize::try_from(meta.len()).map_or(true, |len| len > max_bytes) {
                return Err(StorageError::TooLarge);
            }
            fs::read(&path).map_err(error_of)
        })
        .await
    }

    async fn list(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        let base = root(access);
        let (found, _) = tree::list(
            prefix,
            max,
            crate::passwords::clock::unix_millis(std::time::SystemTime::now()),
            |dir| {
                let base = base.clone();
                async move {
                    let path = resolve(&base, &dir)?;
                    match blocking(deadline, move || read_entries(&path)).await {
                        // A prefix may name no folder at all, as on S3.
                        Err(StorageError::NotFound) => Ok(Vec::new()),
                        other => other,
                    }
                }
            },
        )
        .await?;
        Ok(found)
    }

    /// Deletes a file; a key ending in `/` removes the folder only when it is empty, as deleting a
    /// folder marker leaves the objects under it on S3. A missing file is no error.
    async fn delete(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let path = resolve_below(&root(access), key)?;
        let folder = key.ends_with('/');
        blocking(deadline, move || {
            let removed = if folder {
                fs::remove_dir(&path)
            } else {
                fs::remove_file(&path)
            };
            match removed {
                Err(error)
                    if !matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) =>
                {
                    Err(error_of(error))
                }
                _ => Ok(()),
            }
        })
        .await
    }

    async fn head(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<ObjectHead, StorageError> {
        let path = resolve(&root(access), key)?;
        blocking(deadline, move || {
            let meta = fs::metadata(&path).map_err(error_of)?;
            Ok(ObjectHead {
                size: if meta.is_dir() { 0 } else { meta.len() },
                last_modified: http_date_of(&meta),
                etag: Some(etag_of(&meta)),
            })
        })
        .await
    }

    async fn get_range(
        &self,
        access: &Access,
        key: &str,
        start: u64,
        len: u64,
        deadline: Instant,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, StorageError> {
        let path = resolve(&root(access), key)?;
        let opened = async {
            let mut file = tokio::fs::File::open(&path).await.map_err(error_of)?;
            file.seek(SeekFrom::Start(start)).await.map_err(error_of)?;
            Ok::<_, StorageError>(file)
        };
        let file = tokio::time::timeout_at(deadline, opened)
            .await
            .map_err(|_| StorageError::TimedOut)??;
        Ok(Box::new(file.take(len)))
    }

    async fn list_dir(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<DirListing, StorageError> {
        let path = resolve(&root(access), prefix)?;
        let entries = blocking(deadline, move || read_entries(&path)).await?;
        if entries.len() > max {
            return Err(StorageError::TooLarge);
        }
        let mut listing = DirListing::default();
        for entry in entries {
            if entry.is_dir {
                listing.prefixes.push(format!("{prefix}{}/", entry.name));
            } else {
                listing.objects.push(ObjectInfo {
                    key: format!("{prefix}{}", entry.name),
                    size: entry.size,
                    last_modified: entry.modified,
                    etag: entry.etag,
                });
            }
        }
        listing.prefixes.sort();
        listing.objects.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(listing)
    }

    /// Copies through a temporary file next to `to`.
    async fn copy(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let from = resolve(&root(access), from)?;
        let to = resolve_below(&root(access), to)?;
        blocking(deadline, move || {
            make_parent(&to)?;
            let part = part_of(&to);
            if let Err(error) = fs::copy(&from, &part) {
                let _ = fs::remove_file(&part);
                return Err(error_of(error));
            }
            settle(&part, &to)
        })
        .await
    }

    /// A local copy never streams through holzi.
    fn copies_on_provider(&self, _access: &Access) -> bool {
        true
    }

    async fn rename_tree(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let from = resolve_below(&root(access), from)?;
        let to = resolve_below(&root(access), to)?;
        blocking(deadline, move || {
            make_parent(&to)?;
            fs::rename(&from, &to).map_err(error_of)
        })
        .await
    }

    /// Removes the folder or file `prefix`; a missing one is no error.
    async fn remove_tree(
        &self,
        access: &Access,
        prefix: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let path = resolve_below(&root(access), prefix)?;
        blocking(deadline, move || {
            let removed = match fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_dir() => fs::remove_dir_all(&path),
                Ok(_) => fs::remove_file(&path),
                Err(error) => Err(error),
            };
            match removed {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error_of(error)),
                _ => Ok(()),
            }
        })
        .await
    }

    /// An upload is a temporary file next to `key` that each part is appended to.
    async fn begin_upload(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        let path = resolve_below(&root(access), key)?;
        let part = blocking(deadline, move || {
            make_parent(&path)?;
            let part = part_of(&path);
            fs::File::create(&part).map_err(error_of)?;
            Ok(part)
        })
        .await?;
        let id = Uuid::new_v4().to_string();
        self.uploads
            .lock()
            .expect("uploads")
            .insert(id.clone(), part);
        Ok(id)
    }

    /// Appends a part (in order); the mark is its number.
    async fn upload_part(
        &self,
        _access: &Access,
        _key: &str,
        upload_id: &str,
        number: u16,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        let part = self
            .uploads
            .lock()
            .expect("uploads")
            .get(upload_id)
            .cloned()
            .ok_or(StorageError::NotFound)?;
        blocking(deadline, move || append(&part, &body)).await?;
        Ok(number.to_string())
    }

    async fn finish_upload(
        &self,
        access: &Access,
        key: &str,
        upload_id: &str,
        _marks: &[String],
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let part = self
            .uploads
            .lock()
            .expect("uploads")
            .remove(upload_id)
            .ok_or(StorageError::NotFound)?;
        let path = match resolve_below(&root(access), key) {
            Ok(path) => path,
            Err(error) => {
                let _ = fs::remove_file(&part);
                return Err(error);
            }
        };
        blocking(deadline, move || settle(&part, &path)).await
    }

    async fn abort_upload(
        &self,
        _access: &Access,
        _key: &str,
        upload_id: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let part = self.uploads.lock().expect("uploads").remove(upload_id);
        match part {
            Some(part) => {
                blocking(deadline, move || match fs::remove_file(&part) {
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                        Err(error_of(error))
                    }
                    _ => Ok(()),
                })
                .await
            }
            None => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;

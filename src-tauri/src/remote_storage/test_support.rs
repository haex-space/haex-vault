//! A [`RemoteStore`] in memory for tests (research R10): objects per bucket, an error to answer
//! per operation, and a record of the calls.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use tokio::time::Instant;

use super::{Access, DirListing, ObjectHead, ObjectInfo, RemoteStore, StorageError};

pub use super::test_support_env::{access, credentials, grant_storage, resolver, vault};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Put,
    Get,
    List,
    Delete,
    Head,
    GetRange,
    ListDir,
    Copy,
    RenameTree,
    RemoveTree,
    BeginUpload,
    UploadPart,
    FinishUpload,
    AbortUpload,
}

/// An upload in parts that is not complete yet: bucket, key and its parts by number.
type Upload = (String, String, BTreeMap<u16, Vec<u8>>);

#[derive(Default)]
pub struct FakeStore {
    objects: Mutex<BTreeMap<(String, String), Vec<u8>>>,
    failures: Mutex<HashMap<Op, StorageError>>,
    /// Errors for the next call of an operation only.
    once: Mutex<HashMap<Op, StorageError>>,
    calls: Mutex<Vec<(Op, String)>>,
    /// Key, start and length of every `get_range`.
    ranges: Mutex<Vec<(String, u64, u64)>>,
    uploads: Mutex<HashMap<String, Upload>>,
    /// Modification times other than the default, by bucket and key (ISO 8601).
    times: Mutex<HashMap<(String, String), String>>,
    /// Copies stream through holzi instead of the provider (SFTP, spec 050 research R8).
    no_copy: AtomicBool,
    /// Folders below the connection's root on a WebDAV or SFTP server (spec 050), without `/`.
    folders: Mutex<BTreeSet<String>>,
}

impl FakeStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every later call of `op` answers `error`.
    pub fn fail(&self, op: Op, error: StorageError) {
        self.failures.lock().expect("lock").insert(op, error);
    }

    /// The folder `path` (below the connection's root, without `/`) is on the server.
    pub fn add_folder(&self, path: &str) {
        self.folders.lock().expect("lock").insert(path.to_owned());
    }

    /// Whether the folder `path` is on the server.
    pub fn has_folder(&self, path: &str) -> bool {
        self.folders.lock().expect("lock").contains(path)
    }

    /// The store no longer copies on the provider, as an SFTP server (spec 050 research R8).
    pub fn without_copy(&self) {
        self.no_copy.store(true, Ordering::Relaxed);
    }

    /// Calls of `op` work again.
    pub fn heal(&self, op: Op) {
        self.failures.lock().expect("lock").remove(&op);
    }

    /// The next call of `op` answers `error`, later ones work again.
    pub fn fail_once(&self, op: Op, error: StorageError) {
        self.once.lock().expect("lock").insert(op, error);
    }

    /// The keys stored in `bucket`.
    pub fn keys(&self, bucket: &str) -> Vec<String> {
        self.objects
            .lock()
            .expect("lock")
            .keys()
            .filter(|(b, _)| b == bucket)
            .map(|(_, k)| k.clone())
            .collect()
    }

    /// Stores `body` at `key` in `bucket` without a recorded call.
    pub fn insert(&self, bucket: &str, key: &str, body: &[u8]) {
        self.objects
            .lock()
            .expect("lock")
            .insert((bucket.to_owned(), key.to_owned()), body.to_vec());
    }

    /// Lists the object at `key` in `bucket` as last changed at `iso` (ISO 8601).
    pub fn set_modified(&self, bucket: &str, key: &str, iso: &str) {
        self.times
            .lock()
            .expect("lock")
            .insert((bucket.to_owned(), key.to_owned()), iso.to_owned());
    }

    /// The object at `key` in `bucket`.
    pub fn object(&self, bucket: &str, key: &str) -> Option<Vec<u8>> {
        self.objects
            .lock()
            .expect("lock")
            .get(&(bucket.to_owned(), key.to_owned()))
            .cloned()
    }

    /// The keys of uploads in parts that were started and neither completed nor aborted.
    pub fn open_uploads(&self) -> Vec<String> {
        self.uploads
            .lock()
            .expect("lock")
            .values()
            .map(|(_, key, _)| key.clone())
            .collect()
    }

    /// The operations called so far with their key or prefix.
    pub fn calls(&self) -> Vec<(Op, String)> {
        self.calls.lock().expect("lock").clone()
    }

    /// Key, start and length of every `get_range` so far.
    pub fn ranges(&self) -> Vec<(String, u64, u64)> {
        self.ranges.lock().expect("lock").clone()
    }

    /// Moves the object `from`, or every object under the prefix `from`, to `to` in `bucket`.
    fn move_under(&self, bucket: &str, from: &str, to: &str) -> Result<(), StorageError> {
        let mut objects = self.objects.lock().expect("lock");
        let moved: Vec<(String, String)> = objects
            .keys()
            .filter(|(b, k)| b == bucket && k.starts_with(from))
            .cloned()
            .collect();
        if moved.is_empty() {
            return Err(StorageError::NotFound);
        }
        for slot in moved {
            let body = objects.remove(&slot).expect("listed above");
            objects.insert((slot.0, format!("{to}{}", &slot.1[from.len()..])), body);
        }
        Ok(())
    }

    fn enter(&self, op: Op, key: &str) -> Result<(), StorageError> {
        self.calls.lock().expect("lock").push((op, key.to_owned()));
        if let Some(error) = self.once.lock().expect("lock").remove(&op) {
            return Err(error);
        }
        match self.failures.lock().expect("lock").get(&op) {
            Some(error) => Err(*error),
            None => Ok(()),
        }
    }
}

/// An ETag that changes with the content, as a provider's does.
fn etag(body: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(body);
    format!("\"{}\"", data_encoding::HEXLOWER.encode(&digest[..8]))
}

fn slot(access: &Access, key: &str) -> (String, String) {
    (access.location.bucket.clone(), key.to_owned())
}

#[async_trait]
impl RemoteStore for FakeStore {
    async fn put(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::Put, key)?;
        if access.location.is_tree() && key.ends_with('/') {
            let folder = format!("{}/{key}", access.location.bucket);
            self.add_folder(folder.trim_matches('/'));
        }
        self.objects
            .lock()
            .expect("lock")
            .insert(slot(access, key), body);
        Ok(())
    }

    async fn get(
        &self,
        access: &Access,
        key: &str,
        max_bytes: usize,
        _deadline: Instant,
    ) -> Result<Vec<u8>, StorageError> {
        self.enter(Op::Get, key)?;
        let body = self
            .objects
            .lock()
            .expect("lock")
            .get(&slot(access, key))
            .cloned()
            .ok_or(StorageError::NotFound)?;
        if body.len() > max_bytes {
            return Err(StorageError::TooLarge);
        }
        Ok(body)
    }

    async fn list(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        _deadline: Instant,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        self.enter(Op::List, prefix)?;
        let times = self.times.lock().expect("lock");
        let objects: Vec<ObjectInfo> = self
            .objects
            .lock()
            .expect("lock")
            .iter()
            .filter(|((b, k), _)| *b == access.location.bucket && k.starts_with(prefix))
            .map(|(slot, body)| ObjectInfo {
                key: slot.1.clone(),
                size: body.len() as u64,
                last_modified: times
                    .get(slot)
                    .cloned()
                    .unwrap_or_else(|| "2026-10-06T10:00:00.000Z".to_owned()),
                etag: etag(body),
            })
            .collect();
        if objects.len() > max {
            return Err(StorageError::TooLarge);
        }
        Ok(objects)
    }

    async fn delete(
        &self,
        access: &Access,
        key: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::Delete, key)?;
        self.objects
            .lock()
            .expect("lock")
            .remove(&slot(access, key));
        Ok(())
    }
    async fn head(
        &self,
        access: &Access,
        key: &str,
        _deadline: Instant,
    ) -> Result<ObjectHead, StorageError> {
        self.enter(Op::Head, key)?;
        // The folder of a WebDAV or SFTP storage (the probe asks for it, spec 050 research R10).
        let bucket = &access.location.bucket;
        if access.location.is_tree()
            && key.is_empty()
            && (bucket.is_empty() || self.has_folder(bucket) || self.object(bucket, "").is_some())
        {
            return Ok(ObjectHead {
                size: 0,
                last_modified: None,
                etag: None,
            });
        }
        let (size, etag) = self
            .objects
            .lock()
            .expect("lock")
            .get(&slot(access, key))
            .map(|body| (body.len() as u64, etag(body)))
            .ok_or(StorageError::NotFound)?;
        Ok(ObjectHead {
            size,
            last_modified: Some("Tue, 06 Oct 2026 10:00:00 GMT".to_owned()),
            etag: Some(etag),
        })
    }

    async fn get_range(
        &self,
        access: &Access,
        key: &str,
        start: u64,
        len: u64,
        _deadline: Instant,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, StorageError> {
        self.enter(Op::GetRange, key)?;
        self.ranges
            .lock()
            .expect("lock")
            .push((key.to_owned(), start, len));
        let body = self
            .objects
            .lock()
            .expect("lock")
            .get(&slot(access, key))
            .cloned()
            .ok_or(StorageError::NotFound)?;
        let from = usize::try_from(start).unwrap_or(usize::MAX).min(body.len());
        let to = from
            .saturating_add(usize::try_from(len).unwrap_or(usize::MAX))
            .min(body.len());
        Ok(Box::new(std::io::Cursor::new(body[from..to].to_vec())))
    }

    async fn list_dir(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        _deadline: Instant,
    ) -> Result<DirListing, StorageError> {
        self.enter(Op::ListDir, prefix)?;
        let mut listing = DirListing::default();
        for ((bucket, key), body) in self.objects.lock().expect("lock").iter() {
            if *bucket != access.location.bucket || key == prefix {
                continue;
            }
            let Some(rest) = key.strip_prefix(prefix) else {
                continue;
            };
            match rest.find('/') {
                Some(slash) => {
                    let deeper = format!("{prefix}{}", &rest[..=slash]);
                    if listing.prefixes.last() != Some(&deeper) {
                        listing.prefixes.push(deeper);
                    }
                }
                None => listing.objects.push(ObjectInfo {
                    key: key.clone(),
                    size: body.len() as u64,
                    last_modified: "2026-10-06T10:00:00.000Z".to_owned(),
                    etag: etag(body),
                }),
            }
        }
        listing.prefixes.dedup();
        if listing.objects.len() + listing.prefixes.len() > max {
            return Err(StorageError::TooLarge);
        }
        Ok(listing)
    }

    async fn copy(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::Copy, from)?;
        let mut objects = self.objects.lock().expect("lock");
        let body = objects
            .get(&slot(access, from))
            .cloned()
            .ok_or(StorageError::NotFound)?;
        objects.insert(slot(access, to), body);
        Ok(())
    }

    fn copies_on_provider(&self, _access: &Access) -> bool {
        !self.no_copy.load(Ordering::Relaxed)
    }

    /// Moves every object under `from` (a key or a prefix) to `to`, as one call.
    async fn rename_tree(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::RenameTree, from)?;
        self.move_under(&access.location.bucket, from, to)
    }

    /// Removes every object under `prefix`, as one call.
    async fn remove_tree(
        &self,
        access: &Access,
        prefix: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::RemoveTree, prefix)?;
        let bucket = &access.location.bucket;
        self.objects
            .lock()
            .expect("lock")
            .retain(|(b, k), _| b != bucket || !k.starts_with(prefix));
        Ok(())
    }

    async fn begin_upload(
        &self,
        access: &Access,
        key: &str,
        _deadline: Instant,
    ) -> Result<String, StorageError> {
        self.enter(Op::BeginUpload, key)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.uploads.lock().expect("lock").insert(
            id.clone(),
            (
                access.location.bucket.clone(),
                key.to_owned(),
                BTreeMap::new(),
            ),
        );
        Ok(id)
    }

    async fn upload_part(
        &self,
        _access: &Access,
        key: &str,
        upload_id: &str,
        number: u16,
        body: Vec<u8>,
        _deadline: Instant,
    ) -> Result<String, StorageError> {
        self.enter(Op::UploadPart, key)?;
        let mut uploads = self.uploads.lock().expect("lock");
        let (_, _, parts) = uploads.get_mut(upload_id).ok_or(StorageError::NotFound)?;
        parts.insert(number, body);
        Ok(format!("\"etag-{number}\""))
    }

    async fn finish_upload(
        &self,
        _access: &Access,
        key: &str,
        upload_id: &str,
        etags: &[String],
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::FinishUpload, key)?;
        let (bucket, key, parts) = self
            .uploads
            .lock()
            .expect("lock")
            .remove(upload_id)
            .ok_or(StorageError::NotFound)?;
        if parts.len() != etags.len() {
            return Err(StorageError::Network);
        }
        let body = parts.into_values().flatten().collect();
        self.objects
            .lock()
            .expect("lock")
            .insert((bucket, key), body);
        Ok(())
    }

    async fn abort_upload(
        &self,
        _access: &Access,
        key: &str,
        upload_id: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::AbortUpload, key)?;
        self.uploads.lock().expect("lock").remove(upload_id);
        Ok(())
    }
}

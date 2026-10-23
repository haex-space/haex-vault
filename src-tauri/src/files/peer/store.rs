//! Another own device or a received share as a storage (spec 056 research R2): [`PeerStore`] turns
//! the calls of [`ObjectStore`] into requests of `haex-vault-files/1`, so the file browser, its
//! viewer, transfers, search and agents treat it like any other storage. Keys are window paths
//! without the leading `/`; a folder is a key ending in `/`, as on WebDAV and SFTP.
//!
//! Writing goes through uploads that finish only while the target is as it was when they began
//! (FR-036); a copy streams through holzi, as the serving device copies nothing on its own.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::time::Instant;
use zeroize::Zeroizing;

use crate::files::peer::client::PeerLinks;
use crate::files::peer::exact::ExactBytes;
use crate::files::peer::wire::{
    Caller, Expected, FailCode, Op, Reply, WireEntry, WireKind, WireRoot, MAX_LIST, MAX_PART,
};
use crate::passwords::clock::format_millis;
use crate::remote_storage::{
    Access, Addressing, Credentials, DirListing, EndpointScope, Location, ObjectHead, ObjectInfo,
    ObjectStore, PasswordCredentials, Protocol, ProviderKind, StorageError,
};

/// What a call needs to reach `target` (`peer:<device>` or `share:<id>`) and, for a share, the
/// share's id at its owner; it needs no credentials, so they are empty.
pub fn peer_access(target: &str, share: Option<&str>) -> Access {
    Access {
        location: Location {
            protocol: Protocol::Peer,
            provider_kind: ProviderKind::Other,
            endpoint: target.to_owned(),
            endpoint_scope: EndpointScope::Local,
            region: String::new(),
            addressing: Addressing::Path,
            bucket: share.unwrap_or_default().to_owned(),
            host_key: None,
        },
        credentials: Credentials::Password(PasswordCredentials {
            username: String::new(),
            password: Zeroizing::new(String::new()),
        }),
    }
}

/// The [`ObjectStore`] of other devices.
pub struct PeerStore {
    links: Arc<dyn PeerLinks>,
    caller: Caller,
}

impl PeerStore {
    /// A store whose requests come from `caller` (the user's window or an agent, FR-007).
    pub fn new(links: Arc<dyn PeerLinks>, caller: Caller) -> Self {
        Self { links, caller }
    }

    async fn ask(
        &self,
        access: &Access,
        op: Op,
    ) -> Result<(Reply, iroh::endpoint::RecvStream), StorageError> {
        self.ask_with(access, op, &[]).await
    }

    async fn ask_with(
        &self,
        access: &Access,
        op: Op,
        body: &[u8],
    ) -> Result<(Reply, iroh::endpoint::RecvStream), StorageError> {
        let link = self.links.link(&access.location.endpoint).await?;
        let (reply, recv) = link.ask_with(self.caller, op, body).await?;
        match reply {
            Reply::Failed(code) => Err(storage_error_of(code)),
            reply => Ok((reply, recv)),
        }
    }

    /// Sends `op` before `deadline` and expects `Done`.
    async fn done(&self, access: &Access, op: Op, deadline: Instant) -> Result<(), StorageError> {
        let (reply, _) = tokio::time::timeout_at(deadline, self.ask(access, op))
            .await
            .map_err(|_| StorageError::TimedOut)??;
        match reply {
            Reply::Done | Reply::Deleted { .. } => Ok(()),
            _ => Err(unexpected()),
        }
    }

    /// Reports every change in the folder `prefix` to `changed` until the serving device ends the
    /// watch or the returned stream is dropped (research R16).
    pub async fn watch(
        &self,
        access: &Access,
        prefix: &str,
        deadline: Instant,
    ) -> Result<iroh::endpoint::RecvStream, StorageError> {
        let op = Op::Watch {
            share: share_of(access),
            path: path_of(prefix),
        };
        let (reply, recv) = tokio::time::timeout_at(deadline, self.ask(access, op))
            .await
            .map_err(|_| StorageError::TimedOut)??;
        match reply {
            Reply::Done => Ok(recv),
            _ => Err(unexpected()),
        }
    }

    /// The drives and known places of an own device, by their window paths.
    pub async fn roots(
        &self,
        access: &Access,
        deadline: Instant,
    ) -> Result<(Vec<WireRoot>, Vec<WireRoot>), StorageError> {
        let (reply, _) = tokio::time::timeout_at(deadline, self.ask(access, Op::Shares))
            .await
            .map_err(|_| StorageError::TimedOut)??;
        match reply {
            Reply::Roots { drives, known } => Ok((drives, known)),
            _ => Err(unexpected()),
        }
    }
}

/// The share named by `access`, if it names one.
fn share_of(access: &Access) -> Option<String> {
    (!access.location.bucket.is_empty()).then(|| access.location.bucket.clone())
}

/// The window path of `key`.
fn path_of(key: &str) -> String {
    format!("/{}", key.trim_end_matches('/'))
}

/// A failure the serving device reported, as the storage seam knows it.
pub fn storage_error_of(code: FailCode) -> StorageError {
    match code {
        FailCode::NotFound => StorageError::NotFound,
        FailCode::Refused => StorageError::MissingRight,
        FailCode::Conflict | FailCode::NameTaken => StorageError::Conflict,
        FailCode::TooLarge => StorageError::TooLarge,
        FailCode::Unsupported => StorageError::Unsupported,
        FailCode::SourceUnavailable | FailCode::Busy | FailCode::Internal => StorageError::Network,
    }
}

fn info_of(prefix: &str, entry: &WireEntry) -> ObjectInfo {
    ObjectInfo {
        key: format!("{prefix}{}", entry.name),
        size: entry.size,
        last_modified: format_millis(entry.modified_ms.unwrap_or(0)),
        etag: entry.etag.clone(),
    }
}

fn unexpected() -> StorageError {
    StorageError::Protocol
}

#[async_trait]
impl ObjectStore for PeerStore {
    /// A file as an upload of one part; a key ending in `/` creates the folder.
    async fn put(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        if key.ends_with('/') {
            let op = Op::CreateFolder {
                share: share_of(access),
                path: path_of(key),
            };
            return match self.done(access, op, deadline).await {
                // The folder is there either way.
                Err(StorageError::Conflict) => Ok(()),
                other => other,
            };
        }
        let id = self.begin_upload(access, key, deadline).await?;
        let written = async {
            let mark = self
                .upload_part(access, key, &id, 1, body, deadline)
                .await?;
            self.finish_upload(access, key, &id, &[mark], deadline)
                .await
        };
        let result = written.await;
        if result.is_err() {
            let _ = self.abort_upload(access, key, &id, deadline).await;
        }
        result
    }

    async fn get(
        &self,
        access: &Access,
        key: &str,
        max_bytes: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>, StorageError> {
        let limit = u64::try_from(max_bytes).unwrap_or(u64::MAX);
        let reader = self
            .get_range(access, key, 0, limit.saturating_add(1), deadline)
            .await?;
        let mut bytes = Vec::new();
        reader
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| StorageError::Network)?;
        if bytes.len() > max_bytes {
            return Err(StorageError::TooLarge);
        }
        Ok(bytes)
    }

    /// Every object below `prefix`, folder by folder, at most `max`.
    async fn list(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        let mut found = Vec::new();
        let mut folders = vec![prefix.to_owned()];
        while let Some(folder) = folders.pop() {
            let listing = self.list_dir(access, &folder, max, deadline).await?;
            found.extend(listing.objects);
            folders.extend(listing.prefixes);
            if found.len() > max {
                return Err(StorageError::TooLarge);
            }
        }
        Ok(found)
    }

    /// Deletes a file; a folder goes with [`ObjectStore::remove_tree`], as on every server with
    /// real folders.
    async fn delete(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        if key.ends_with('/') {
            return Err(StorageError::Unsupported);
        }
        let op = Op::Delete {
            share: share_of(access),
            path: path_of(key),
        };
        match self.done(access, op, deadline).await {
            Err(StorageError::NotFound) => Ok(()),
            other => other,
        }
    }

    async fn head(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<ObjectHead, StorageError> {
        let op = Op::Stat {
            share: share_of(access),
            path: path_of(key),
        };
        let (reply, _) = tokio::time::timeout_at(deadline, self.ask(access, op))
            .await
            .map_err(|_| StorageError::TimedOut)??;
        let Reply::Entry(entry) = reply else {
            return Err(unexpected());
        };
        Ok(ObjectHead {
            size: entry.size,
            last_modified: entry.modified_ms.map(|ms| {
                let at = std::time::UNIX_EPOCH
                    + std::time::Duration::from_millis(u64::try_from(ms).unwrap_or(0));
                httpdate::fmt_http_date(at)
            }),
            etag: (!entry.etag.is_empty()).then_some(entry.etag),
        })
    }

    async fn get_range(
        &self,
        access: &Access,
        key: &str,
        start: u64,
        len: u64,
        deadline: Instant,
    ) -> Result<Box<dyn AsyncRead + Send + Unpin>, StorageError> {
        let op = Op::Read {
            share: share_of(access),
            path: path_of(key),
            start,
            len,
        };
        let (reply, recv) = tokio::time::timeout_at(deadline, self.ask(access, op))
            .await
            .map_err(|_| StorageError::TimedOut)??;
        let Reply::Reading { len, .. } = reply else {
            return Err(unexpected());
        };
        // No deadline from here on: a film plays for hours (FR-012).
        Ok(Box::new(ExactBytes::new(recv.take(len), len)))
    }

    async fn list_dir(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<DirListing, StorageError> {
        let op = Op::ListDir {
            share: share_of(access),
            path: path_of(prefix),
            max: u32::try_from(max).unwrap_or(MAX_LIST).min(MAX_LIST),
        };
        let (reply, _) = tokio::time::timeout_at(deadline, self.ask(access, op))
            .await
            .map_err(|_| StorageError::TimedOut)??;
        let Reply::Entries(entries) = reply else {
            return Err(unexpected());
        };
        let mut listing = DirListing::default();
        for entry in &entries {
            match entry.kind {
                WireKind::File => listing.objects.push(info_of(prefix, entry)),
                WireKind::Folder => listing.prefixes.push(format!("{prefix}{}/", entry.name)),
            }
        }
        Ok(listing)
    }

    async fn copy(&self, _: &Access, _: &str, _: &str, _: Instant) -> Result<(), StorageError> {
        Err(StorageError::Unsupported)
    }

    fn copies_on_provider(&self, _: &Access) -> bool {
        false
    }

    async fn rename_tree(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let op = Op::Move {
            share: share_of(access),
            from: path_of(from),
            to: path_of(to),
        };
        self.done(access, op, deadline).await
    }

    async fn remove_tree(
        &self,
        access: &Access,
        prefix: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let op = Op::Delete {
            share: share_of(access),
            path: path_of(prefix),
        };
        match self.done(access, op, deadline).await {
            Err(StorageError::NotFound) => Ok(()),
            other => other,
        }
    }

    /// Starts an upload that finishes only while `key` is still as it is now (FR-036): absent, or
    /// at the version its head shows.
    async fn begin_upload(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        let expected = match self.head(access, key, deadline).await {
            Ok(head) => Expected::Version {
                size: head.size,
                etag: head.etag.unwrap_or_default(),
            },
            Err(StorageError::NotFound) => Expected::Absent,
            Err(error) => return Err(error),
        };
        let op = Op::BeginUpload {
            share: share_of(access),
            path: path_of(key),
            expected,
        };
        let (reply, _) = tokio::time::timeout_at(deadline, self.ask(access, op))
            .await
            .map_err(|_| StorageError::TimedOut)??;
        match reply {
            Reply::Upload { upload_id } => Ok(upload_id),
            _ => Err(unexpected()),
        }
    }

    async fn upload_part(
        &self,
        access: &Access,
        _key: &str,
        upload_id: &str,
        number: u16,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        if body.len() as u64 > MAX_PART {
            return Err(StorageError::TooLarge);
        }
        let op = Op::UploadPart {
            upload_id: upload_id.to_owned(),
            number,
            len: body.len() as u64,
        };
        let (reply, _) = tokio::time::timeout_at(deadline, self.ask_with(access, op, &body))
            .await
            .map_err(|_| StorageError::TimedOut)??;
        match reply {
            Reply::Mark(mark) => Ok(mark),
            _ => Err(unexpected()),
        }
    }

    /// The serving device keeps the parts in order; the marks are not needed there.
    async fn finish_upload(
        &self,
        access: &Access,
        _key: &str,
        upload_id: &str,
        _marks: &[String],
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let op = Op::FinishUpload {
            upload_id: upload_id.to_owned(),
        };
        self.done(access, op, deadline).await
    }

    async fn abort_upload(
        &self,
        access: &Access,
        _key: &str,
        upload_id: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let op = Op::AbortUpload {
            upload_id: upload_id.to_owned(),
        };
        self.done(access, op, deadline).await
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

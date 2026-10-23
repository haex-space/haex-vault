//! Another own device or a received share as a storage (spec 056 research R2): [`PeerStore`] turns
//! the calls of [`ObjectStore`] into requests of `haex-vault-files/1`, so the file browser, its
//! viewer, transfers, search and agents treat it like any other storage. Keys are window paths
//! without the leading `/`; a folder is a key ending in `/`, as on WebDAV and SFTP.
//!
//! This delivery reads; writing comes with the delivery for own devices and answers
//! [`StorageError::Unsupported`] until then.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::time::Instant;
use zeroize::Zeroizing;

use crate::files::peer::client::PeerLinks;
use crate::files::peer::wire::{Caller, FailCode, Op, Reply, WireEntry, WireKind, MAX_LIST};
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
        let link = self.links.link(&access.location.endpoint).await?;
        let (reply, recv) = link.ask(self.caller, op).await?;
        match reply {
            Reply::Failed(code) => Err(storage_error_of(code)),
            reply => Ok((reply, recv)),
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
        FailCode::Conflict => StorageError::Conflict,
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
    async fn put(&self, _: &Access, _: &str, _: Vec<u8>, _: Instant) -> Result<(), StorageError> {
        Err(StorageError::Unsupported)
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

    async fn delete(&self, _: &Access, _: &str, _: Instant) -> Result<(), StorageError> {
        Err(StorageError::Unsupported)
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
        Ok(Box::new(recv.take(len)))
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
        _: &Access,
        _: &str,
        _: &str,
        _: Instant,
    ) -> Result<(), StorageError> {
        Err(StorageError::Unsupported)
    }

    async fn remove_tree(&self, _: &Access, _: &str, _: Instant) -> Result<(), StorageError> {
        Err(StorageError::Unsupported)
    }

    async fn begin_upload(&self, _: &Access, _: &str, _: Instant) -> Result<String, StorageError> {
        Err(StorageError::Unsupported)
    }

    async fn upload_part(
        &self,
        _: &Access,
        _: &str,
        _: &str,
        _: u16,
        _: Vec<u8>,
        _: Instant,
    ) -> Result<String, StorageError> {
        Err(StorageError::Unsupported)
    }

    async fn finish_upload(
        &self,
        _: &Access,
        _: &str,
        _: &str,
        _: &[String],
        _: Instant,
    ) -> Result<(), StorageError> {
        Err(StorageError::Unsupported)
    }

    async fn abort_upload(
        &self,
        _: &Access,
        _: &str,
        _: &str,
        _: Instant,
    ) -> Result<(), StorageError> {
        Err(StorageError::Unsupported)
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

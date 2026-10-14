//! WebDAV, written by holzi on `reqwest` and `quick-xml` (spec 050 research R2, contracts/
//! object-store.md): a storage is a folder on the server, a key a path below it. Files are written
//! to a temporary file next to their name and moved onto it, so a cancel never leaves half a file
//! (FR-016); folders are real folders and show in listings as S3 shows its folder markers.
//!
//! Every request goes to the addresses [`super::address::pin`] checked, follows no redirect and
//! signs in with Basic (`https`, or `http` in the local network). At most a few requests per
//! connection run at once (research R12).

mod multistatus;
#[cfg(test)]
mod multistatus_tests;
mod ops;
mod request;
#[cfg(test)]
mod request_tests;
#[cfg(test)]
mod server_tests;
#[cfg(test)]
pub(crate) mod test_server;
mod upload;
#[cfg(test)]
mod upload_tests;
#[cfg(test)]
mod webdav_tests;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use reqwest::{Method, StatusCode};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

use super::address::{Resolver, SystemResolver};
use super::tree::{self, parent_of, part_name};
use super::{Access, Credentials, DirListing, ObjectHead, ObjectInfo, ObjectStore, StorageError};
use request::{method, send, url_of};
use upload::Sessions;

/// Requests that may run at once against one connection (research R12).
const PER_CONNECTION: usize = 4;

/// The current time in milliseconds since 1970, to tell the age of temporary files.
fn now_ms() -> i64 {
    crate::passwords::clock::unix_millis(std::time::SystemTime::now())
}

/// The WebDAV implementation of [`ObjectStore`].
pub struct WebDavStore {
    resolver: Arc<dyn Resolver>,
    sessions: Sessions,
    /// One semaphore per server and user.
    lanes: Mutex<HashMap<String, Arc<Semaphore>>>,
}

impl WebDavStore {
    /// A WebDAV store that checks and pins addresses with `resolver`.
    pub fn new(resolver: Arc<dyn Resolver>) -> Self {
        Self {
            resolver,
            sessions: Sessions::default(),
            lanes: Mutex::new(HashMap::new()),
        }
    }

    /// With the resolver of the operating system.
    pub fn system() -> Self {
        Self::new(Arc::new(SystemResolver))
    }

    /// A permit to send one request to the connection of `access`, before `deadline`.
    async fn permit(
        &self,
        access: &Access,
        deadline: Instant,
    ) -> Result<OwnedSemaphorePermit, StorageError> {
        let user = match &access.credentials {
            Credentials::Password(login) => login.username.as_str(),
            _ => "",
        };
        let lane = Arc::clone(
            self.lanes
                .lock()
                .expect("lanes")
                .entry(format!("{user}@{}", access.location.endpoint))
                .or_insert_with(|| Arc::new(Semaphore::new(PER_CONNECTION))),
        );
        tokio::time::timeout_at(deadline, lane.acquire_owned())
            .await
            .map_err(|_| StorageError::TimedOut)?
            .map_err(|_| StorageError::Network)
    }

    /// Writes `body` to a temporary file in the folder of `key`, creating missing folders below
    /// the storage's folder once, and returns the key of the temporary file.
    async fn put_part(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        let part = format!("{}{}", parent_of(key), part_name());
        for attempt in 0..2 {
            let status = {
                let _permit = self.permit(access, deadline).await?;
                request::send_raw(
                    self.resolver.as_ref(),
                    access,
                    Method::PUT,
                    url_of(&access.location, &part)?,
                    &[],
                    Some(body.clone().into()),
                    deadline,
                )
                .await?
                .status()
            };
            match status {
                status if status.is_success() => return Ok(part),
                StatusCode::CONFLICT | StatusCode::NOT_FOUND if attempt == 0 => {
                    self.make_parents(access, key, deadline).await?;
                }
                status => return Err(request::status_error(&Method::PUT, status)),
            }
        }
        Err(StorageError::NotFound)
    }

    /// Moves the temporary file `part` onto `key`; on failure the temporary file is deleted.
    async fn settle(
        &self,
        access: &Access,
        part: &str,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let moved = self
            .transfer(access, method("MOVE"), part, key, deadline)
            .await;
        if moved.is_err() {
            let _ = self.remove(access, part, deadline).await;
        }
        moved
    }
}

#[async_trait]
impl ObjectStore for WebDavStore {
    /// A file through a temporary file; a key ending in `/` creates the folder.
    async fn put(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        if key.ends_with('/') {
            self.make_parents(access, key, deadline).await?;
            return self.make_folder(access, key, deadline).await;
        }
        let part = self.put_part(access, key, body, deadline).await?;
        self.settle(access, &part, key, deadline).await
    }

    async fn get(
        &self,
        access: &Access,
        key: &str,
        max_bytes: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>, StorageError> {
        let sent = {
            let _permit = self.permit(access, deadline).await?;
            send(
                self.resolver.as_ref(),
                access,
                Method::GET,
                url_of(&access.location, key)?,
                &[],
                None,
                deadline,
            )
            .await
        };
        let response = match sent {
            Ok(response) => response,
            Err(error) => return Err(self.file_error(access, key, error, deadline).await),
        };
        super::http::read_body(response, max_bytes, deadline).await
    }

    async fn list(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        let (found, stale) = tree::list(prefix, max, now_ms(), |dir| async move {
            self.read_dir(access, &dir, deadline).await
        })
        .await?;
        self.sweep(access, stale, deadline).await;
        Ok(found)
    }

    /// Deletes a file; a key ending in `/` removes the folder only when it is empty, as deleting a
    /// folder marker leaves the objects under it on S3.
    async fn delete(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        if key.ends_with('/') {
            let empty = match self.read_dir(access, key, deadline).await {
                Ok(entries) => entries.is_empty(),
                Err(StorageError::NotFound) => return Ok(()),
                Err(error) => return Err(error),
            };
            if !empty {
                return Ok(());
            }
        }
        self.remove(access, key, deadline).await
    }

    async fn head(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<ObjectHead, StorageError> {
        let entries = match self.propfind(access, key, 0, deadline).await {
            Ok(entries) => entries,
            Err(error) => return Err(self.file_error(access, key, error, deadline).await),
        };
        let entry = entries
            .into_iter()
            .find(|entry| entry.name.is_empty())
            .ok_or(StorageError::NotFound)?;
        Ok(ObjectHead {
            size: entry.size,
            last_modified: entry.modified,
            etag: entry.etag,
        })
    }

    async fn get_range(
        &self,
        access: &Access,
        key: &str,
        start: u64,
        len: u64,
        deadline: Instant,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, StorageError> {
        if len == 0 {
            return Ok(Box::new(tokio::io::empty()));
        }
        // The permit covers the start of the answer; the body then streams without it.
        let sent = {
            let _permit = self.permit(access, deadline).await?;
            send(
                self.resolver.as_ref(),
                access,
                Method::GET,
                url_of(&access.location, key)?,
                &[("range", format!("bytes={start}-{}", start + len - 1))],
                None,
                deadline,
            )
            .await
        };
        let response = match sent {
            Ok(response) => response,
            Err(error) => return Err(self.file_error(access, key, error, deadline).await),
        };
        // A server that ignores the range answers 200 with the whole file; only a part from the
        // start can be cut from that (as for S3).
        if response.status() == StatusCode::OK && start > 0 {
            log::warn!("remote storage: a WebDAV range request was answered with the whole file");
            return Err(StorageError::Network);
        }
        let stream = futures::TryStreamExt::map_err(response.bytes_stream(), |_| {
            std::io::Error::other("the server's answer broke off")
        });
        let reader = tokio_util::io::StreamReader::new(stream);
        Ok(Box::new(tokio::io::AsyncReadExt::take(reader, len)))
    }

    async fn list_dir(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<DirListing, StorageError> {
        let entries = self.read_dir(access, prefix, deadline).await?;
        let (listing, stale) = tree::level(prefix, entries, max, now_ms())?;
        self.sweep(access, stale, deadline).await;
        Ok(listing)
    }

    async fn copy(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        self.transfer(access, method("COPY"), from, to, deadline)
            .await
    }

    /// WebDAV copies on the server (`COPY`).
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
        self.transfer(access, method("MOVE"), from, to, deadline)
            .await
    }

    async fn remove_tree(
        &self,
        access: &Access,
        prefix: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        self.remove(access, prefix, deadline).await
    }

    async fn begin_upload(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        self.make_parents(access, key, deadline).await?;
        let part = format!("{}{}", parent_of(key), part_name());
        self.sessions
            .begin(Arc::clone(&self.resolver), access, part)
    }

    /// Sends a part into the upload; the mark is its number, WebDAV has none of its own.
    async fn upload_part(
        &self,
        _access: &Access,
        _key: &str,
        upload_id: &str,
        number: u16,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        self.sessions.part(upload_id, body, deadline).await?;
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
        let (part, outcome) = self
            .sessions
            .finish(upload_id, deadline)
            .await
            .ok_or(StorageError::NotFound)?;
        if let Err(error) = outcome {
            let _ = self.remove(access, &part, deadline).await;
            return Err(error);
        }
        self.settle(access, &part, key, deadline).await
    }

    async fn abort_upload(
        &self,
        access: &Access,
        _key: &str,
        upload_id: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        match self.sessions.abort(upload_id) {
            Some(part) => self.remove(access, &part, deadline).await,
            None => Ok(()),
        }
    }
}

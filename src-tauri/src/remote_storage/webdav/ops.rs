//! The building blocks of [`super::WebDavStore`] (spec 050 research R2, R7): reading a folder or a
//! file with `PROPFIND`, creating missing folders below the folder of the storage, and moving or
//! copying with `Overwrite: T`. Each takes a permit of its connection (research R12).

use reqwest::{Method, StatusCode};
use tokio::time::Instant;

use super::super::tree::{ancestors, DirEntry};
use super::super::{Access, StorageError, StorageSpace};
use super::multistatus::{self, Entry};
use super::request::{
    decoded_path, method, send, send_raw, status_error, url_of, PROPFIND_BODY, QUOTA_BODY,
};
use super::WebDavStore;
use crate::files::storage_time::parse_http_date_ms;
use crate::passwords::clock::format_millis;

/// The largest `PROPFIND` answer holzi reads (about 100 000 entries).
const MAX_LISTING: usize = 64 * 1024 * 1024;

/// The entry of a `PROPFIND` as one entry of its folder: the HTTP date as the time of an S3
/// listing; a server without `getetag` gets a mark from size and time.
pub(super) fn dir_entry(entry: Entry) -> DirEntry {
    let sent = entry.modified.unwrap_or_default();
    let modified = parse_http_date_ms(&sent)
        .map(format_millis)
        .unwrap_or_default();
    let etag = entry
        .etag
        .unwrap_or_else(|| format!("{}-{sent}", entry.size));
    DirEntry {
        name: entry.name,
        is_dir: entry.is_dir,
        size: entry.size,
        modified,
        etag,
    }
}

impl WebDavStore {
    /// `PROPFIND` of `key` (a file, or a folder with `/` at the end; empty for the folder of the
    /// storage) with `depth` 0 or 1.
    pub(super) async fn propfind(
        &self,
        access: &Access,
        key: &str,
        depth: u8,
        deadline: Instant,
    ) -> Result<Vec<Entry>, StorageError> {
        self.propfind_with(access, key, depth, PROPFIND_BODY, deadline)
            .await
    }

    /// What the server says about the quota of the folder of the storage (RFC 4331): used bytes,
    /// and with the bytes still available the total. A server that sends nothing, or a negative
    /// number for "unlimited" or "unknown", gives no total; one that sends nothing at all gives
    /// `None`.
    pub(super) async fn quota(
        &self,
        access: &Access,
        deadline: Instant,
    ) -> Result<Option<StorageSpace>, StorageError> {
        let entries = self
            .propfind_with(access, "", 0, QUOTA_BODY, deadline)
            .await?;
        let Some(entry) = entries.into_iter().find(|entry| entry.name.is_empty()) else {
            return Ok(None);
        };
        let Some(used) = entry.quota_used else {
            return Ok(None);
        };
        let total = entry
            .quota_available
            .and_then(|available| u64::try_from(available).ok())
            .and_then(|available| used.checked_add(available));
        Ok(Some(StorageSpace { used, total }))
    }

    /// `PROPFIND` of `key` with the request `body`.
    async fn propfind_with(
        &self,
        access: &Access,
        key: &str,
        depth: u8,
        body: &'static str,
        deadline: Instant,
    ) -> Result<Vec<Entry>, StorageError> {
        let _permit = self.permit(access, deadline).await?;
        let url = url_of(&access.location, key)?;
        let requested = decoded_path(&url);
        let response = send(
            self.resolver.as_ref(),
            access,
            method("PROPFIND"),
            url,
            &[
                ("depth", depth.to_string()),
                ("content-type", "application/xml; charset=utf-8".to_owned()),
            ],
            Some(body.into()),
            deadline,
        )
        .await?;
        if response.status() != StatusCode::MULTI_STATUS {
            log::warn!(
                "remote storage: WebDAV PROPFIND answered {} instead of 207",
                response.status().as_u16()
            );
            return Err(StorageError::Protocol);
        }
        let body = super::super::http::read_body(response, MAX_LISTING, deadline).await?;
        let xml = std::str::from_utf8(&body).map_err(|_| StorageError::Protocol)?;
        multistatus::parse(xml, &requested)
    }

    /// The entries of the folder `dir` (a key ending in `/`, or empty), without the folder itself.
    pub(super) async fn read_dir(
        &self,
        access: &Access,
        dir: &str,
        deadline: Instant,
    ) -> Result<Vec<DirEntry>, StorageError> {
        Ok(self
            .propfind(access, dir, 1, deadline)
            .await?
            .into_iter()
            .filter(|entry| !entry.name.is_empty() && !entry.name.contains('/'))
            .map(dir_entry)
            .collect())
    }

    /// Creates the folders of `key` below the folder of the storage, from the top; one that is
    /// there already is fine. A missing folder of the storage itself is [`StorageError::NotFound`]:
    /// holzi creates it only on the user's word (FR-006).
    pub(super) async fn make_parents(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        for folder in ancestors(key) {
            self.make_folder(access, folder, deadline).await?;
        }
        Ok(())
    }

    /// `MKCOL` of the folder `folder` (ending in `/`); 405 means it is there.
    pub(super) async fn make_folder(
        &self,
        access: &Access,
        folder: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let _permit = self.permit(access, deadline).await?;
        let mkcol = method("MKCOL");
        let url = url_of(&access.location, folder)?;
        let response = send_raw(
            self.resolver.as_ref(),
            access,
            mkcol.clone(),
            url,
            &[],
            None,
            deadline,
        )
        .await?;
        match response.status() {
            status if status.is_success() => Ok(()),
            StatusCode::METHOD_NOT_ALLOWED => Ok(()),
            status => Err(status_error(&mkcol, status)),
        }
    }

    /// `MOVE` or `COPY` of `from` to `to` with `Overwrite: T`; a missing folder of `to` is created
    /// and the call made once more.
    pub(super) async fn transfer(
        &self,
        access: &Access,
        verb: Method,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        for attempt in 0..2 {
            let status = {
                let _permit = self.permit(access, deadline).await?;
                let destination = url_of(&access.location, to)?;
                send_raw(
                    self.resolver.as_ref(),
                    access,
                    verb.clone(),
                    url_of(&access.location, from)?,
                    &[
                        ("destination", destination.to_string()),
                        ("overwrite", "T".to_owned()),
                    ],
                    None,
                    deadline,
                )
                .await?
                .status()
            };
            match status {
                status if status.is_success() => return Ok(()),
                StatusCode::CONFLICT if attempt == 0 => {
                    self.make_parents(access, to, deadline).await?;
                }
                status => return Err(status_error(&verb, status)),
            }
        }
        Err(StorageError::NotFound)
    }

    /// Deletes `key` (a file, or a folder with everything in it); one that is gone is fine.
    pub(super) async fn remove(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let _permit = self.permit(access, deadline).await?;
        let response = send_raw(
            self.resolver.as_ref(),
            access,
            Method::DELETE,
            url_of(&access.location, key)?,
            &[],
            None,
            deadline,
        )
        .await?;
        match response.status() {
            status if status.is_success() => Ok(()),
            StatusCode::NOT_FOUND => Ok(()),
            status => Err(status_error(&Method::DELETE, status)),
        }
    }

    /// Deletes temporary files a broken upload left behind (older than a day, found by a
    /// listing); a failure is only logged, the next listing tries again.
    pub(super) async fn sweep(&self, access: &Access, stale: Vec<String>, deadline: Instant) {
        if stale.is_empty() {
            return;
        }
        let mut failed = 0;
        for key in &stale {
            if self.remove(access, key, deadline).await.is_err() {
                failed += 1;
            }
        }
        log::info!(
            "remote storage: deleted {} of {} old WebDAV temporary files",
            stale.len() - failed,
            stale.len()
        );
    }

    /// `error` of a call on the file `key`, or [`StorageError::NotFound`] when the server
    /// redirected because a folder has that name (servers send `x` on to `x/`): no file has that
    /// key, as on S3.
    pub(super) async fn file_error(
        &self,
        access: &Access,
        key: &str,
        error: StorageError,
        deadline: Instant,
    ) -> StorageError {
        let folder = format!("{key}/");
        if error == StorageError::Protocol
            && !key.is_empty()
            && !key.ends_with('/')
            && self.propfind(access, &folder, 0, deadline).await.is_ok()
        {
            return StorageError::NotFound;
        }
        error
    }
}

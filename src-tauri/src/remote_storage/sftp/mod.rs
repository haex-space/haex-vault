//! SFTP, spoken by holzi through `russh` and `russh-sftp` (spec 050 research R3–R5, contracts/
//! object-store.md): a storage is a folder on the server, a key a path below it. The host key
//! must be the confirmed one before any login (FR-012); files are written to a temporary file and
//! renamed onto their name (FR-016); folders are real folders and show in listings as S3 shows
//! its folder markers. Sessions are shared between calls ([`pool`]). A copy streams through holzi:
//! SFTP has no copy on the server that holzi can count on (research R8).

mod connect;
pub mod keys;
mod ops;
mod pool;
mod rename;
mod upload;

#[cfg(test)]
mod connect_tests;
#[cfg(test)]
mod keys_tests;
#[cfg(test)]
mod pool_tests;
#[cfg(test)]
mod sftp_tests;
#[cfg(test)]
pub(crate) mod test_server;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::time::Instant;

use super::address::{Resolver, SystemResolver};
use super::tree::{self, parent_of, part_name};
use super::{
    Access, DirListing, Location, ObjectHead, ObjectInfo, ObjectStore, StorageError, StorageSpace,
};
use connect::{sftp_error, Session};
use ops::{path_of, read_dir, space_of, stamp};
use pool::Pool;
use upload::Uploads;

/// The current time in milliseconds since 1970, to tell the age of temporary files.
fn now_ms() -> i64 {
    crate::passwords::clock::unix_millis(std::time::SystemTime::now())
}

/// An HTTP date of seconds since 1970, as a `HEAD` of S3 gives the time of an object.
fn http_date(seconds: u32) -> String {
    let iso = crate::passwords::clock::format_millis(i64::from(seconds) * 1000);
    let days = i64::from(seconds) / 86_400;
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = iso[5..7].parse::<usize>().unwrap_or(1);
    format!(
        "{}, {} {} {} {} GMT",
        WEEKDAYS[days.rem_euclid(7) as usize],
        &iso[8..10],
        MONTHS[month - 1],
        &iso[0..4],
        &iso[11..19]
    )
}

/// The SFTP implementation of [`ObjectStore`].
pub struct SftpStore {
    resolver: Arc<dyn Resolver>,
    pool: Pool,
    uploads: Uploads,
    /// The host key each endpoint showed when a connection was refused for it (research R5).
    shown: Mutex<HashMap<String, String>>,
}

impl SftpStore {
    /// An SFTP store that checks and pins addresses with `resolver`.
    pub fn new(resolver: Arc<dyn Resolver>) -> Self {
        Self {
            resolver,
            pool: Pool::default(),
            uploads: Uploads::default(),
            shown: Mutex::new(HashMap::new()),
        }
    }

    /// With the resolver of the operating system.
    pub fn system() -> Self {
        Self::new(Arc::new(SystemResolver))
    }

    /// A session to the server of `access`, opened before `deadline` when none is free.
    async fn session(
        &self,
        access: &Access,
        deadline: Instant,
    ) -> Result<Arc<Session>, StorageError> {
        let opened = self
            .pool
            .session(access, || {
                connect::connect(self.resolver.as_ref(), access, deadline)
            })
            .await;
        opened.map_err(|refused| {
            if let Some(key) = refused.shown_key {
                self.shown
                    .lock()
                    .expect("shown keys")
                    .insert(access.location.endpoint.clone(), key);
            }
            refused.error
        })
    }

    /// Runs `work` on a session before `deadline`.
    async fn within<T, F, Fut>(
        &self,
        access: &Access,
        deadline: Instant,
        work: F,
    ) -> Result<T, StorageError>
    where
        F: FnOnce(Arc<Session>) -> Fut,
        Fut: std::future::Future<Output = Result<T, StorageError>>,
    {
        let session = self.session(access, deadline).await?;
        tokio::time::timeout_at(deadline, work(session))
            .await
            .unwrap_or(Err(StorageError::TimedOut))
    }

    /// Deletes temporary files a broken upload left behind; a failure is only logged.
    async fn sweep(&self, session: &Session, access: &Access, stale: Vec<String>) {
        let mut failed = 0;
        for key in &stale {
            let removed = match path_of(&access.location, key) {
                Ok(path) => session.sftp.remove_file(path).await.is_ok(),
                Err(_) => false,
            };
            if !removed {
                failed += 1;
            }
        }
        if !stale.is_empty() {
            log::info!(
                "remote storage: deleted {} of {} old SFTP temporary files",
                stale.len() - failed,
                stale.len()
            );
        }
    }
}

#[async_trait]
impl ObjectStore for SftpStore {
    /// A file through a temporary file; a key ending in `/` creates the folder.
    async fn put(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let location = &access.location;
        self.within(access, deadline, |session| async move {
            if key.ends_with('/') {
                ops::put_folder(&session, location, key).await
            } else {
                ops::put_file(&session, location, key, &body).await
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
        let path = path_of(&access.location, key)?;
        self.within(access, deadline, |session| async move {
            let attrs = session
                .sftp
                .metadata(path.clone())
                .await
                .map_err(sftp_error)?;
            if attrs.is_dir() {
                return Err(StorageError::NotFound);
            }
            if attrs.len() > max_bytes as u64 {
                return Err(StorageError::TooLarge);
            }
            session.sftp.read(path).await.map_err(sftp_error)
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
        let location = &access.location;
        self.within(access, deadline, |session| async move {
            let (found, stale) = tree::list(prefix, max, now_ms(), |dir| {
                let session = Arc::clone(&session);
                async move { read_dir(&session, location, &dir).await }
            })
            .await?;
            self.sweep(&session, access, stale).await;
            Ok(found)
        })
        .await
    }

    /// Deletes a file; a key ending in `/` removes the folder only when it is empty, as deleting a
    /// folder marker leaves the objects under it on S3.
    async fn delete(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let location = &access.location;
        let path = path_of(location, key)?;
        self.within(access, deadline, |session| async move {
            let removed = if key.ends_with('/') {
                match read_dir(&session, location, key).await {
                    Ok(entries) if entries.is_empty() => {
                        session.sftp.remove_dir(path).await.map_err(sftp_error)
                    }
                    Ok(_) => Ok(()),
                    Err(error) => Err(error),
                }
            } else {
                session.sftp.remove_file(path).await.map_err(sftp_error)
            };
            match removed {
                Err(StorageError::NotFound) => Ok(()),
                other => other,
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
        let path = path_of(&access.location, key)?;
        self.within(access, deadline, |session| async move {
            let attrs = session.sftp.metadata(path).await.map_err(sftp_error)?;
            // A folder answers only for the folder of the storage itself (the probe asks for it).
            if attrs.is_dir() && !key.is_empty() {
                return Err(StorageError::NotFound);
            }
            let (_, etag) = stamp(&attrs);
            Ok(ObjectHead {
                size: if attrs.is_dir() { 0 } else { attrs.len() },
                last_modified: attrs.mtime.map(http_date),
                etag: Some(etag),
            })
        })
        .await
    }

    async fn space(
        &self,
        access: &Access,
        deadline: Instant,
    ) -> Result<Option<StorageSpace>, StorageError> {
        let path = path_of(&access.location, "")?;
        self.within(access, deadline, |session| async move {
            // A server without `statvfs@openssh.com` answers `None`: it cannot tell.
            let Some(info) = session.sftp.fs_info(path).await.map_err(sftp_error)? else {
                return Ok(None);
            };
            Ok(space_of(info.fragment_size, info.blocks, info.blocks_avail))
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
        if len == 0 {
            return Ok(Box::new(tokio::io::empty()));
        }
        let path = path_of(&access.location, key)?;
        self.within(access, deadline, |session| async move {
            let mut file = session.sftp.open(path).await.map_err(sftp_error)?;
            file.seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(|_| StorageError::Network)?;
            let reader: Box<dyn tokio::io::AsyncRead + Send + Unpin> = Box::new(file.take(len));
            Ok(reader)
        })
        .await
    }

    async fn list_dir(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<DirListing, StorageError> {
        let location = &access.location;
        self.within(access, deadline, |session| async move {
            let entries = read_dir(&session, location, prefix).await?;
            let (listing, stale) = tree::level(prefix, entries, max, now_ms())?;
            self.sweep(&session, access, stale).await;
            Ok(listing)
        })
        .await
    }

    /// SFTP has no copy holzi can count on; [`ObjectStore::copies_on_provider`] says so.
    async fn copy(
        &self,
        _access: &Access,
        _from: &str,
        _to: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        Err(StorageError::Unsupported)
    }

    fn copies_on_provider(&self, _access: &Access) -> bool {
        false
    }

    fn shown_host_key(&self, location: &Location) -> Option<String> {
        self.shown
            .lock()
            .expect("shown keys")
            .get(&location.endpoint)
            .cloned()
    }

    async fn rename_tree(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let location = &access.location;
        let (from_path, to_path) = (path_of(location, from)?, path_of(location, to)?);
        self.within(access, deadline, |session| async move {
            ops::make_parents(&session, location, to).await?;
            rename::rename_new(&session, &from_path, &to_path).await
        })
        .await
    }

    async fn remove_tree(
        &self,
        access: &Access,
        prefix: &str,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let path = path_of(&access.location, prefix)?;
        self.within(access, deadline, |session| async move {
            ops::remove_tree(&session, &path).await
        })
        .await
    }

    async fn begin_upload(
        &self,
        access: &Access,
        key: &str,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        let location = &access.location;
        let part = path_of(location, &format!("{}{}", parent_of(key), part_name()))?;
        let target = path_of(location, key)?;
        self.within(access, deadline, |session| async move {
            ops::make_parents(&session, location, key).await?;
            self.uploads.begin(session, part, target).await
        })
        .await
    }

    /// Writes a part at its offset; the mark is its number, SFTP has none of its own.
    async fn upload_part(
        &self,
        _access: &Access,
        _key: &str,
        upload_id: &str,
        number: u16,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<String, StorageError> {
        self.uploads.part(upload_id, number, body, deadline).await?;
        Ok(number.to_string())
    }

    async fn finish_upload(
        &self,
        _access: &Access,
        _key: &str,
        upload_id: &str,
        _marks: &[String],
        deadline: Instant,
    ) -> Result<(), StorageError> {
        self.uploads.finish(upload_id, deadline).await
    }

    async fn abort_upload(
        &self,
        _access: &Access,
        _key: &str,
        upload_id: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.uploads.abort(upload_id).await
    }
}

//! Sending bytes to a storage key (spec 044 FR-020): one `put` for what fits into one [`PART`],
//! else an upload in parts that starts with the first full part. Every part is kept until the
//! provider took it, so a retry sends the same bytes again; that also holds the format rule of
//! spec 048 that a sealed block is never sealed twice. A cancel or a failure aborts the upload, so
//! no part stays at the provider.

use std::sync::Arc;

use super::{soon, Run, Side, Stop, PART};
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::remote_plan::fill;
use crate::files::{FilesError, FilesErrorCode};

/// An upload to one key: in parts once the first part was sent.
pub(super) struct Upload {
    key: String,
    id: Option<String>,
    etags: Vec<String>,
}

impl Upload {
    pub(super) fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            id: None,
            etags: Vec::new(),
        }
    }
}

impl Run<'_, '_> {
    pub(super) fn target_files(&self) -> Arc<StorageFiles> {
        match &self.job.to {
            Side::Storage(files) => Arc::clone(files),
            Side::Device => unreachable!("an upload goes to a storage"),
        }
    }

    /// Sends what `read` gives to `key`.
    pub(super) fn upload(
        &mut self,
        key: &str,
        read: &mut dyn FnMut(&mut [u8]) -> std::io::Result<usize>,
    ) -> Result<(), Stop> {
        let files = self.target_files();
        let mut upload = Upload::new(key);
        let sent = (|| loop {
            self.check()?;
            let body = fill(read, PART).map_err(|_| {
                Stop::Failed(FilesError::new(FilesErrorCode::NoAccess, "not readable"))
            })?;
            let length = body.len() as u64;
            if body.len() < PART {
                self.finish_upload(&files, &mut upload, body)?;
                self.done.bytes += length;
                return Ok(());
            }
            self.send_part(&files, &mut upload, body)?;
            self.done.bytes += length;
            self.report();
        })();
        if sent.is_err() {
            self.abort_upload(&files, &upload);
        }
        sent
    }

    /// Sends one full part; the first one starts the upload in parts. The caller counts the bytes.
    pub(super) fn send_part(
        &mut self,
        files: &StorageFiles,
        upload: &mut Upload,
        body: Vec<u8>,
    ) -> Result<(), Stop> {
        let (store, access, key) = (files.store(), files.access(), upload.key.as_str());
        let id = match &upload.id {
            Some(id) => id.clone(),
            None => {
                let id = self.retried(|| {
                    self.handle
                        .block_on(store.begin_upload(access, key, soon()))
                })?;
                upload.id = Some(id.clone());
                id
            }
        };
        let number = u16::try_from(upload.etags.len() + 1).map_err(|_| {
            Stop::Failed(FilesError::new(FilesErrorCode::TooLarge, "too many parts"))
        })?;
        let etag = self.retried(|| {
            self.handle
                .block_on(store.upload_part(access, key, &id, number, body.clone(), soon()))
        })?;
        upload.etags.push(etag);
        Ok(())
    }

    /// Sends the last bytes: the whole object in one `put`, or the last part and the join.
    pub(super) fn finish_upload(
        &mut self,
        files: &StorageFiles,
        upload: &mut Upload,
        last: Vec<u8>,
    ) -> Result<(), Stop> {
        let (store, access, key) = (files.store(), files.access(), upload.key.clone());
        if upload.id.is_none() {
            return self.retried(|| {
                self.handle
                    .block_on(store.put(access, &key, last.clone(), soon()))
            });
        }
        if !last.is_empty() {
            self.send_part(files, upload, last)?;
        }
        self.check()?;
        let id = upload.id.clone().unwrap_or_default();
        self.retried(|| {
            self.handle
                .block_on(store.finish_upload(access, &key, &id, &upload.etags, soon()))
        })
    }

    /// Drops a started upload in parts (FR-020).
    pub(super) fn abort_upload(&self, files: &StorageFiles, upload: &Upload) {
        if let Some(id) = &upload.id {
            let _ = self.handle.block_on(files.store().abort_upload(
                files.access(),
                &upload.key,
                id,
                soon(),
            ));
        }
    }
}

//! Uploads in parts to an SFTP server (spec 050 research R7, R10): a temporary file next to the
//! target, each part written at its offset (a retried part lands where its first try went), then
//! the file is closed and renamed onto its name. A cancel removes the temporary file. When a write
//! fails, holzi asks the server how much space is free (`statvfs@openssh.com`, when offered) to
//! tell a full storage from a broken connection. Sessions without a part for 30 minutes go.

use std::collections::{BTreeMap, HashMap};
use std::io::SeekFrom;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh_sftp::client::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::time::Instant;
use uuid::Uuid;

use super::super::StorageError;
use super::connect::{sftp_error, Session};
use super::rename::rename_over;

/// An upload without a part for this long is dropped.
const IDLE: Duration = Duration::from_secs(30 * 60);

struct Upload {
    session: Arc<Session>,
    /// Path of the temporary file and of the target.
    part: String,
    target: String,
    file: Arc<tokio::sync::Mutex<File>>,
    /// Offset and length of each part written, by number.
    parts: BTreeMap<u16, (u64, u64)>,
    touched: Instant,
}

/// The open uploads by id.
#[derive(Default)]
pub(super) struct Uploads {
    open: Mutex<HashMap<String, Upload>>,
}

impl Uploads {
    /// Creates the temporary file `part` for the file `target`; returns the upload's id.
    pub async fn begin(
        &self,
        session: Arc<Session>,
        part: String,
        target: String,
    ) -> Result<String, StorageError> {
        let file = session
            .sftp
            .create(part.clone())
            .await
            .map_err(sftp_error)?;
        let id = Uuid::new_v4().to_string();
        let mut open = self.open.lock().expect("uploads");
        open.retain(|_, upload| upload.touched.elapsed() < IDLE);
        open.insert(
            id.clone(),
            Upload {
                session,
                part,
                target,
                file: Arc::new(tokio::sync::Mutex::new(file)),
                parts: BTreeMap::new(),
                touched: Instant::now(),
            },
        );
        Ok(id)
    }

    /// Writes part `number` at its offset: where it went before when it is tried again, else
    /// right after the part before it.
    pub async fn part(
        &self,
        id: &str,
        number: u16,
        body: Vec<u8>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let (session, file, offset) = {
            let mut open = self.open.lock().expect("uploads");
            let upload = open.get_mut(id).ok_or(StorageError::NotFound)?;
            upload.touched = Instant::now();
            let offset = match upload.parts.get(&number) {
                Some((offset, _)) => *offset,
                None => upload
                    .parts
                    .range(..number)
                    .next_back()
                    .map_or(0, |(_, (offset, len))| offset + len),
            };
            (
                Arc::clone(&upload.session),
                Arc::clone(&upload.file),
                offset,
            )
        };
        let len = body.len() as u64;
        let written = tokio::time::timeout_at(deadline, async {
            let mut file = file.lock().await;
            file.seek(SeekFrom::Start(offset)).await?;
            file.write_all(&body).await?;
            file.flush().await
        })
        .await;
        match written {
            Ok(Ok(())) => {
                if let Some(upload) = self.open.lock().expect("uploads").get_mut(id) {
                    upload.parts.insert(number, (offset, len));
                }
                Ok(())
            }
            Ok(Err(_)) => Err(full_or_broken(&session, id, self, len).await),
            Err(_) => Err(StorageError::TimedOut),
        }
    }

    /// Closes the temporary file and renames it onto the target; a failure removes it.
    pub async fn finish(&self, id: &str, deadline: Instant) -> Result<(), StorageError> {
        let upload = self
            .open
            .lock()
            .expect("uploads")
            .remove(id)
            .ok_or(StorageError::NotFound)?;
        let finished = tokio::time::timeout_at(deadline, async {
            upload
                .file
                .lock()
                .await
                .shutdown()
                .await
                .map_err(|_| StorageError::Network)?;
            rename_over(&upload.session, &upload.part, &upload.target).await
        })
        .await
        .unwrap_or(Err(StorageError::TimedOut));
        if finished.is_err() {
            let _ = upload.session.sftp.remove_file(upload.part.clone()).await;
        }
        finished
    }

    /// Ends an upload without a file; an unknown one is done already.
    pub async fn abort(&self, id: &str) -> Result<(), StorageError> {
        let Some(upload) = self.open.lock().expect("uploads").remove(id) else {
            return Ok(());
        };
        drop(upload.file);
        match upload
            .session
            .sftp
            .remove_file(upload.part)
            .await
            .map_err(sftp_error)
        {
            Ok(()) | Err(StorageError::NotFound) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

/// After a failed write of `len` bytes: [`StorageError::QuotaExceeded`] when the server says less
/// space is free, else [`StorageError::Network`].
async fn full_or_broken(session: &Session, id: &str, uploads: &Uploads, len: u64) -> StorageError {
    if !session.extensions.statvfs {
        return StorageError::Network;
    }
    let part = uploads
        .open
        .lock()
        .expect("uploads")
        .get(id)
        .map(|upload| upload.part.clone());
    let Some(part) = part else {
        return StorageError::Network;
    };
    match session.raw.statvfs(part).await {
        Ok(space) if space.blocks_avail.saturating_mul(space.fragment_size) < len => {
            StorageError::QuotaExceeded
        }
        _ => StorageError::Network,
    }
}

//! The uploads of one connection (spec 056 FR-035, FR-036, FR-044): each writes a temporary file
//! next to its target through [`LocalStore`](crate::remote_storage::local::LocalStore) and puts it
//! in place only while the target is still as the other side saw it. They belong to the
//! connection: when it ends, every one left is dropped with its temporary file.

use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use super::ops::fail_of;
use super::roots::Located;
use super::ServeContext;
use crate::files::peer::wire::{Expected, FailCode};
use crate::remote_storage::local::access_of;
use crate::remote_storage::{ObjectStore, StorageError};

/// Most uploads one connection runs at once.
const UPLOADS_PER_CONNECTION: usize = 16;
/// How long writing one part or putting the file in place may take.
const STEP_LIMIT: Duration = Duration::from_secs(120);
/// How long the bytes of one part may take to arrive.
pub const PART_LIMIT: Duration = Duration::from_secs(300);

fn deadline() -> Instant {
    Instant::now() + STEP_LIMIT
}

/// One running upload: what it was asked for, so every later request is checked again (FR-005).
#[derive(Debug, Clone)]
pub struct Upload {
    pub share: Option<String>,
    pub path: String,
    expected: Expected,
    at: Located,
    target_lock: Arc<tokio::sync::Mutex<()>>,
}

/// The running uploads of a connection, by id.
#[derive(Default)]
pub struct Uploads {
    map: Mutex<HashMap<String, Upload>>,
    closed: AtomicBool,
}

impl Uploads {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, Upload>> {
        self.map.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The upload `id` of this connection.
    pub fn get(&self, id: &str) -> Option<Upload> {
        self.map().get(id).cloned()
    }

    /// Starts writing `at` (asked as `share` and `path`); refused at once when the target already
    /// differs from `expected`.
    pub async fn begin(
        &self,
        ctx: &ServeContext,
        at: Located,
        share: Option<String>,
        path: String,
        expected: Expected,
    ) -> Result<String, FailCode> {
        if self.map().len() >= UPLOADS_PER_CONNECTION {
            return Err(FailCode::Busy);
        }
        let target_lock = ctx.write_lock(&at.root.join(&at.key));
        let _target_guard = target_lock.lock().await;
        check(ctx, &at, &expected).await?;
        let root = at.root.clone();
        let key = at.key.clone();
        let id = ctx
            .store
            .begin_upload(&access_of(&at.root), &at.key, deadline())
            .await
            .map_err(fail_of)?;
        let closed = {
            let mut uploads = self.map();
            if self.closed.load(Ordering::Acquire) {
                true
            } else {
                uploads.insert(
                    id.clone(),
                    Upload {
                        share,
                        path,
                        expected,
                        at,
                        target_lock,
                    },
                );
                false
            }
        };
        if closed {
            let _ = ctx
                .store
                .abort_upload(&access_of(&root), &key, &id, deadline())
                .await;
            return Err(FailCode::Internal);
        }
        Ok(id)
    }

    /// Appends part `number` of the upload `id`; its mark.
    pub async fn part(
        &self,
        ctx: &ServeContext,
        id: &str,
        number: u16,
        body: Vec<u8>,
    ) -> Result<String, FailCode> {
        let upload = self.get(id).ok_or(FailCode::NotFound)?;
        ctx.store
            .upload_part(
                &access_of(&upload.at.root),
                &upload.at.key,
                id,
                number,
                body,
                deadline(),
            )
            .await
            .map_err(fail_of)
    }

    /// Puts the file of upload `id` in place, unless its target changed since the other side saw
    /// it: then the upload is dropped, the target stays as it is, and the answer is `Conflict`.
    pub async fn finish(&self, ctx: &ServeContext, id: &str) -> Result<(), FailCode> {
        let upload = self.map().remove(id).ok_or(FailCode::NotFound)?;
        let _target_guard = upload.target_lock.lock().await;
        let access = access_of(&upload.at.root);
        if let Err(code) = check(ctx, &upload.at, &upload.expected).await {
            let _ = ctx
                .store
                .abort_upload(&access, &upload.at.key, id, deadline())
                .await;
            return Err(code);
        }
        match upload.expected {
            Expected::Absent => ctx
                .store
                .finish_upload_if_absent(&access, &upload.at.key, id, deadline())
                .await
                .map_err(fail_of),
            Expected::Version { .. } => ctx
                .store
                .finish_upload(&access, &upload.at.key, id, &[], deadline())
                .await
                .map_err(fail_of),
        }
    }

    /// Drops the upload `id` with its temporary file.
    pub async fn abort(&self, ctx: &ServeContext, id: &str) -> Result<(), FailCode> {
        let Some(upload) = self.map().remove(id) else {
            return Ok(());
        };
        ctx.store
            .abort_upload(&access_of(&upload.at.root), &upload.at.key, id, deadline())
            .await
            .map_err(fail_of)
    }

    /// Drops every upload left, at the end of the connection.
    pub async fn abort_all(&self, ctx: &ServeContext) {
        self.closed.store(true, Ordering::Release);
        let ids: Vec<String> = self.map().keys().cloned().collect();
        for id in ids {
            let _ = self.abort(ctx, &id).await;
        }
    }
}

/// Whether the target `at` is still as `expected`.
async fn check(ctx: &ServeContext, at: &Located, expected: &Expected) -> Result<(), FailCode> {
    let head = ctx
        .store
        .head(&access_of(&at.root), &at.key, deadline())
        .await;
    let same = match (expected, head) {
        (Expected::Absent, Err(StorageError::NotFound)) => true,
        (Expected::Version { size, etag }, Ok(head)) => {
            head.size == *size && head.etag.as_deref() == Some(etag.as_str())
        }
        (_, Err(StorageError::NotFound)) | (_, Ok(_)) => false,
        (_, Err(error)) => return Err(fail_of(error)),
    };
    if same {
        Ok(())
    } else {
        Err(FailCode::Conflict)
    }
}

#[cfg(test)]
#[path = "upload_tests.rs"]
mod tests;

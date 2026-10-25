//! The uploads of one connection (spec 056 FR-035, FR-036, FR-044): on this device each writes a
//! temporary file next to its target through [`LocalStore`](crate::remote_storage::local::LocalStore);
//! into a storage the parts go on to the provider ([`super::storage_upload`], T081). A file is put
//! in place only while the target is still as the other side saw it. Uploads belong to the
//! connection: when it ends, every one left is dropped with what it sent.

use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use super::ops::fail_of;
use super::storage::version;
use super::storage_upload::StorageUpload;
use super::target::Target;
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

/// What an upload into a storage sent so far; taken when it ends.
type Sink = Arc<tokio::sync::Mutex<Option<StorageUpload>>>;

/// One running upload: what it was asked for, so every later request is checked again (FR-005).
#[derive(Clone)]
pub struct Upload {
    pub share: Option<String>,
    pub path: String,
    expected: Expected,
    target: Target,
    /// Set for an upload into a storage.
    sink: Option<Sink>,
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

    /// Starts writing `target` (asked as `share` and `path`); refused at once when the target
    /// already differs from `expected`.
    pub async fn begin(
        &self,
        ctx: &ServeContext,
        target: Target,
        share: Option<String>,
        path: String,
        expected: Expected,
    ) -> Result<String, FailCode> {
        if self.map().len() >= UPLOADS_PER_CONNECTION {
            return Err(FailCode::Busy);
        }
        let target_lock = ctx.write_lock(&target.lock_key());
        let _target_guard = target_lock.lock().await;
        check(ctx, &target, &expected).await?;
        let (id, sink) = match &target {
            Target::Device(at) => (
                ctx.store
                    .begin_upload(&access_of(&at.root), &at.key, deadline())
                    .await
                    .map_err(fail_of)?,
                None,
            ),
            Target::Storage(at) => (
                uuid::Uuid::new_v4().to_string(),
                Some(Arc::new(tokio::sync::Mutex::new(Some(
                    StorageUpload::begin((**at).clone()).await?,
                )))),
            ),
        };
        drop(_target_guard);
        let upload = Upload {
            share,
            path,
            expected,
            target,
            sink,
            target_lock,
        };
        let refused = {
            let mut uploads = self.map();
            if self.closed.load(Ordering::Acquire) {
                Some(upload)
            } else {
                uploads.insert(id.clone(), upload);
                None
            }
        };
        if let Some(upload) = refused {
            drop_upload(ctx, &id, upload).await;
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
        match (&upload.target, &upload.sink) {
            (Target::Device(at), _) => ctx
                .store
                .upload_part(&access_of(&at.root), &at.key, id, number, body, deadline())
                .await
                .map_err(fail_of),
            (Target::Storage(_), Some(sink)) => {
                let mut sink = sink.lock().await;
                let running = sink.as_mut().ok_or(FailCode::NotFound)?;
                running.part(body).await?;
                Ok(number.to_string())
            }
            (Target::Storage(_), None) => Err(FailCode::Internal),
        }
    }

    /// Puts the file of upload `id` in place, unless its target changed since the other side saw
    /// it: then the upload is dropped, the target stays as it is, and the answer is `Conflict`.
    pub async fn finish(&self, ctx: &ServeContext, id: &str) -> Result<(), FailCode> {
        let upload = self.map().remove(id).ok_or(FailCode::NotFound)?;
        let target_lock = Arc::clone(&upload.target_lock);
        let _target_guard = target_lock.lock().await;
        if let Err(code) = check(ctx, &upload.target, &upload.expected).await {
            drop_upload(ctx, id, upload).await;
            return Err(code);
        }
        match (&upload.target, &upload.sink) {
            (Target::Device(at), _) => {
                let access = access_of(&at.root);
                match upload.expected {
                    Expected::Absent => ctx
                        .store
                        .finish_upload_if_absent(&access, &at.key, id, deadline())
                        .await
                        .map_err(fail_of),
                    Expected::Version { .. } => ctx
                        .store
                        .finish_upload(&access, &at.key, id, &[], deadline())
                        .await
                        .map_err(fail_of),
                }
            }
            (Target::Storage(_), Some(sink)) => {
                let running = sink.lock().await.take().ok_or(FailCode::NotFound)?;
                running.finish().await
            }
            (Target::Storage(_), None) => Err(FailCode::Internal),
        }
    }

    /// Drops the upload `id` with its temporary file.
    pub async fn abort(&self, ctx: &ServeContext, id: &str) -> Result<(), FailCode> {
        let Some(upload) = self.map().remove(id) else {
            return Ok(());
        };
        drop_upload(ctx, id, upload).await;
        Ok(())
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

/// Drops what the upload `id` sent so far.
async fn drop_upload(ctx: &ServeContext, id: &str, upload: Upload) {
    match (&upload.target, upload.sink) {
        (Target::Device(at), _) => {
            let _ = ctx
                .store
                .abort_upload(&access_of(&at.root), &at.key, id, deadline())
                .await;
        }
        (Target::Storage(_), Some(sink)) => {
            if let Some(running) = sink.lock().await.take() {
                running.abort().await;
            }
        }
        (Target::Storage(_), None) => {}
    }
}

/// Whether the target is still as `expected`.
async fn check(ctx: &ServeContext, target: &Target, expected: &Expected) -> Result<(), FailCode> {
    let now = match target {
        Target::Device(at) => match ctx
            .store
            .head(&access_of(&at.root), &at.key, deadline())
            .await
        {
            Ok(head) => Some((head.size, head.etag.unwrap_or_default())),
            Err(StorageError::NotFound) => None,
            Err(error) => return Err(fail_of(error)),
        },
        Target::Storage(at) => version(&at.files, &at.path()).await?,
    };
    let same = match (expected, now) {
        (Expected::Absent, None) => true,
        (Expected::Version { size, etag }, Some((now_size, now_etag))) => {
            now_size == *size && now_etag == *etag
        }
        _ => false,
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

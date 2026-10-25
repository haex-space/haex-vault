//! Where a checked request goes: a path on this device, or a path in a storage a share lies in
//! (T081). The operations here pass it to [`super::ops`] and [`super::change`] or to
//! [`super::storage`]; a storage that fails as a whole marks its share's source unavailable
//! (FR-022).

use std::path::PathBuf;

use tokio::io::AsyncRead;

use super::roots::Located;
use super::storage::{self, InStorage};
use super::{change, ops, ServeContext};
use crate::files::peer::wire::{FailCode, WireEntry};
use crate::files::share::{served, FileShareSourceState};
use crate::files::SourceRef;

/// A request's target after every check.
#[derive(Clone)]
pub enum Target {
    Device(Located),
    Storage(Box<InStorage>),
}

impl Target {
    /// The key below the root: `""` is the root itself.
    pub fn key(&self) -> &str {
        match self {
            Self::Device(at) => &at.key,
            Self::Storage(at) => &at.at.key,
        }
    }

    /// What peer writes to the same target lock on.
    pub fn lock_key(&self) -> PathBuf {
        match self {
            Self::Device(at) => at.root.join(&at.key),
            Self::Storage(at) => {
                let storage = match &at.share.source {
                    SourceRef::Storage { storage_id } => storage_id.as_str(),
                    SourceRef::Device => "",
                };
                PathBuf::from(format!("storage:{storage}{}", at.path()))
            }
        }
    }
}

/// Notes what `result` says of the source of a storage's share: reached, its root gone, or the
/// storage out of reach.
async fn noted<T>(
    ctx: &ServeContext,
    at: &InStorage,
    result: Result<T, FailCode>,
) -> Result<T, FailCode> {
    let found = match &result {
        Ok(_) => Some(FileShareSourceState::Ok),
        Err(FailCode::SourceUnavailable) => Some(FileShareSourceState::Unavailable),
        Err(FailCode::NotFound) if at.at.key.is_empty() => Some(FileShareSourceState::Missing),
        Err(_) => None,
    };
    if let Some(found) = found {
        served::note_state(&ctx.db, &at.share, found).await;
    }
    result
}

pub async fn list(
    ctx: &ServeContext,
    target: &Target,
    max: usize,
    hide_own: bool,
) -> Result<Vec<WireEntry>, FailCode> {
    match target {
        Target::Device(at) => ops::list(ctx, at, max, hide_own).await,
        Target::Storage(at) => noted(ctx, at, storage::list(at, max).await).await,
    }
}

pub async fn stat(ctx: &ServeContext, target: &Target) -> Result<WireEntry, FailCode> {
    match target {
        Target::Device(at) => ops::stat(ctx, at).await,
        Target::Storage(at) => noted(ctx, at, storage::stat(at).await).await,
    }
}

pub async fn open_read(
    ctx: &ServeContext,
    target: &Target,
    start: u64,
    len: u64,
) -> Result<(u64, u64, Box<dyn AsyncRead + Send + Unpin>), FailCode> {
    match target {
        Target::Device(at) => ops::open_read(ctx, at, start, len).await,
        Target::Storage(at) => noted(ctx, at, storage::open_read(at, start, len).await).await,
    }
}

pub async fn create_folder(ctx: &ServeContext, target: &Target) -> Result<(), FailCode> {
    match target {
        Target::Device(at) => change::create_folder(at).await,
        Target::Storage(at) => noted(ctx, at, storage::create_folder(at).await).await,
    }
}

pub async fn move_entry(ctx: &ServeContext, from: &Target, to: &Target) -> Result<(), FailCode> {
    match (from, to) {
        (Target::Device(from), Target::Device(to)) => change::move_entry(from, to).await,
        (Target::Storage(from), Target::Storage(to)) => {
            noted(ctx, from, storage::move_entry(from, to).await).await
        }
        _ => Err(FailCode::Unsupported),
    }
}

/// Removes the entry; whether it went to the trash (a storage has none).
pub async fn delete(ctx: &ServeContext, target: &Target) -> Result<bool, FailCode> {
    match target {
        Target::Device(at) => change::delete(ctx, at).await,
        Target::Storage(at) => noted(ctx, at, storage::delete(at).await.map(|()| false)).await,
    }
}

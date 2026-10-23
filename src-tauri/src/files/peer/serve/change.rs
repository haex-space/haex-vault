//! The changes on the serving device (spec 056 FR-012, FR-037, research R8): a new folder, a move
//! below the same root and a removal, into the trash where the device has one. An entry is named
//! by its folder, resolved inside the root, and its own name, so a link is moved or removed itself
//! and never what it points to.

use std::path::{Path, PathBuf};

use super::ops::fail_of;
use super::roots::Located;
use super::ServeContext;
use crate::files::peer::wire::FailCode;
use crate::files::transfer::entries;
use crate::files::transfer::local::Removal;
use crate::files::{FilesError, FilesErrorCode};
use crate::remote_storage::local_fs::{error_of, resolve};

/// The folder of `at`, resolved inside its root, and the entry's name.
fn entry_of(at: &Located) -> Result<PathBuf, FailCode> {
    let (folder, name) = match at.key.rsplit_once('/') {
        Some((folder, name)) => (folder, name),
        None => ("", at.key.as_str()),
    };
    if name.is_empty() {
        return Err(FailCode::Refused);
    }
    let folder = resolve(&at.root, folder).map_err(fail_of)?;
    Ok(folder.join(name))
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, FailCode> + Send + 'static,
) -> Result<T, FailCode> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| FailCode::Internal)?
}

/// Creates the folder `at`; an existing entry of that name stays and answers `NameTaken`.
pub async fn create_folder(at: &Located) -> Result<(), FailCode> {
    let path = entry_of(at)?;
    blocking(move || match std::fs::create_dir(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(FailCode::NameTaken),
        Err(error) => Err(fail_of(error_of(error))),
    })
    .await
}

/// Moves `from` to `to`, both below the same root; an existing `to` stays and answers `NameTaken`.
pub async fn move_entry(from: &Located, to: &Located) -> Result<(), FailCode> {
    if from.root != to.root {
        return Err(FailCode::Unsupported);
    }
    let (from, to) = (entry_of(from)?, entry_of(to)?);
    blocking(move || {
        std::fs::symlink_metadata(&from).map_err(|error| fail_of(error_of(error)))?;
        // A rename that changes only case names the same entry on some file systems.
        let same = same_entry(&from, &to);
        if !same && std::fs::symlink_metadata(&to).is_ok() {
            return Err(FailCode::NameTaken);
        }
        std::fs::rename(&from, &to).map_err(|error| fail_of(error_of(error)))
    })
    .await
}

/// Whether two paths name the same entry, including on case-sensitive file systems.
fn same_entry(a: &Path, b: &Path) -> bool {
    match (std::fs::symlink_metadata(a), std::fs::symlink_metadata(b)) {
        #[cfg(unix)]
        (Ok(a), Ok(b)) => {
            use std::os::unix::fs::MetadataExt;
            a.dev() == b.dev() && a.ino() == b.ino()
        }
        #[cfg(not(unix))]
        (Ok(_), Ok(_)) => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
        _ => false,
    }
}

/// Removes the entry `at` as `ctx` removes; whether it went to the trash.
pub async fn delete(ctx: &ServeContext, at: &Located) -> Result<bool, FailCode> {
    let path = entry_of(at)?;
    let how = ctx.removal;
    blocking(move || {
        std::fs::symlink_metadata(&path).map_err(|error| fail_of(error_of(error)))?;
        remove(&path, how)
    })
    .await?;
    Ok(how == Removal::Trash)
}

fn remove(path: &Path, how: Removal) -> Result<(), FailCode> {
    entries::remove(path, how).map_err(|error: FilesError| match error.code {
        FilesErrorCode::NotFound => FailCode::NotFound,
        FilesErrorCode::NoAccess | FilesErrorCode::HolziOwned => FailCode::Refused,
        _ => FailCode::SourceUnavailable,
    })
}

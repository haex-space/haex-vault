//! Files of the device for extensions (spec 017, US9, FR-046–FR-049, FR-066, research R19): the
//! SDK's `extension_filesystem_*` methods, all in Rust.
//!
//! Every path is checked at its real target ([`resolve`]), against a fixed list of holzi's own
//! places first ([`FsEnvironment::denied`], FR-049), then against what the user chose in a dialog
//! from this frame ([`FsState::grant`], FR-048), then against the extension's `filesystem`
//! permissions, which may ask the user (1004). On Android and iOS only dialog choices hold; a free
//! path and watching answer "not available" (8001).

pub mod dialogs;
pub mod ops;
pub mod resolve;
pub mod watch;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use serde_json::json;
use uuid::Uuid;

use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::{
    evaluate, Action, Decision, PermissionKind, PermissionRequest, RequestTarget,
};
use crate::files::encrypted::device::has_raw_part;
use crate::files::local::{known_places, OwnPlaces};
use crate::files::picked::{Opener, PickedFile};
use crate::vault_gate::VaultDb;

pub use dialogs::FileDialogs;
pub use resolve::resolve;

pub const MODULE: &str = module_path!();

/// What a call does with a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

impl Access {
    fn action(self) -> Action {
        match self {
            Self::Read => Action::Read,
            Self::Write => Action::ReadWrite,
        }
    }
}

/// How far a call reaches below its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// The path itself.
    Path,
    /// The path and everything below it (remove recursively, rename, copy, watch).
    Tree,
}

/// holzi's places and helpers on this device, set once at start.
pub struct FsEnvironment {
    /// holzi's own data, vaults, configuration, cache and logs: never reachable (FR-049).
    pub denied: OwnPlaces,
    /// The SDK's known places (`home`, `documents`, …) that exist here.
    pub known: Vec<(&'static str, PathBuf)>,
    pub dialogs: Arc<dyn FileDialogs>,
    /// Where `open_file` and `show_image` put a file for the system viewer.
    pub scratch: PathBuf,
    /// Whether free paths exist on this platform (not on Android and iOS, FR-066).
    pub free_paths: bool,
    /// How a chosen document without a path (Android: a `content://` address) is read and
    /// written: the file plugin (spec 043 FR-022).
    pub opener: Arc<dyn Opener + Send + Sync>,
}

/// The environment of the running app: holzi's own app folders are protected (resolved, so a link
/// cannot hide them), the SDK's known places that exist are offered, and opened copies go to a
/// folder in holzi's cache, itself protected.
pub fn environment_for<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> FsEnvironment {
    use tauri::Manager;
    let paths = app.path();
    let denied = OwnPlaces::from_app(app);
    let known = known_places(app)
        .into_iter()
        .map(|place| (place.name, place.path))
        .collect();
    let scratch = paths.app_cache_dir().map_or_else(
        |_| std::env::temp_dir().join("holzi-extension-files"),
        |dir| dir.join("extension-files"),
    );
    dialogs::prune_scratch(&scratch);
    FsEnvironment {
        denied,
        known,
        dialogs: Arc::new(dialogs::TauriDialogs(app.clone())),
        scratch,
        free_paths: crate::platform::capabilities().free_paths,
        opener: Arc::new(app.clone()),
    }
}

/// A file or folder the user chose in a dialog of one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Choice {
    path: PathBuf,
    access: Access,
    reach: Reach,
}

impl Choice {
    fn covers(&self, path: &Path, access: Access) -> bool {
        let inside = match self.reach {
            Reach::Path => path == self.path,
            Reach::Tree => path.starts_with(&self.path),
        };
        inside && (access == Access::Read || self.access == Access::Write)
    }
}

/// The file state of the extension host.
#[derive(Default)]
pub struct FsState {
    environment: RwLock<Option<Arc<FsEnvironment>>>,
    /// Dialog choices per frame; they end with the frame (FR-048).
    choices: Mutex<HashMap<String, Vec<Choice>>>,
    /// Chosen documents without a path per frame (spec 043 FR-022: on Android the system hands a
    /// `content://` address): the frame reads or writes exactly these through the file plugin and
    /// never learns a path. They end with the frame too.
    documents: Mutex<HashMap<String, Vec<(PickedFile, Access)>>>,
    pub(crate) watches: watch::Watches,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl FsState {
    pub fn set_environment(&self, environment: FsEnvironment) {
        *self
            .environment
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(environment));
    }

    /// The environment; a host without one (never in the app) offers no files.
    pub fn environment(&self) -> Result<Arc<FsEnvironment>, BridgeError> {
        self.environment
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(BridgeError::not_available)
    }

    /// Lets `frame` reach `path` as the user chose it.
    fn grant(&self, frame: &str, path: PathBuf, access: Access, reach: Reach) {
        lock(&self.choices)
            .entry(frame.to_owned())
            .or_default()
            .push(Choice {
                path,
                access,
                reach,
            });
    }

    /// Lets `frame` reach the chosen document `file`.
    fn grant_document(&self, frame: &str, file: PickedFile, access: Access) {
        lock(&self.documents)
            .entry(frame.to_owned())
            .or_default()
            .push((file, access));
    }

    /// The document `raw` names, if `frame` chose it for `access`.
    pub fn document(&self, frame: &str, raw: &str, access: Access) -> Option<PickedFile> {
        lock(&self.documents).get(frame).and_then(|documents| {
            documents
                .iter()
                .find(|(file, granted)| {
                    file.0 == raw && (access == Access::Read || *granted == Access::Write)
                })
                .map(|(file, _)| file.clone())
        })
    }

    fn chosen(&self, frame: &str, path: &Path, access: Access) -> bool {
        lock(&self.choices)
            .get(frame)
            .is_some_and(|choices| choices.iter().any(|c| c.covers(path, access)))
    }

    /// Forgets the choices of a closed frame and ends the watches only they allowed; with its
    /// extension's last frame all its watches end.
    pub fn frame_closed(&self, frame: &str, extension_id: Uuid, last_frame: bool) {
        lock(&self.choices).remove(frame);
        lock(&self.documents).remove(frame);
        self.watches.end_frame(frame);
        if last_frame {
            self.watches.end_all(extension_id);
        }
    }
}

fn protected() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::PermissionDenied, "path is protected")
}

/// Whether `path` touches one of holzi's own places (lies in one, or, for a call that reaches the
/// whole tree, holds one) or goes through the raw directory of an encrypted folder, which an
/// extension reaches only through that folder's own permission (spec 048 FR-051). A tree call on
/// a folder that holds an encrypted folder is not refused: it moves or removes the whole folder,
/// as any other folder of the user, and never leaves it half.
fn touches_denied(environment: &FsEnvironment, path: &Path, reach: Reach) -> bool {
    environment.denied.touches(path, reach == Reach::Tree) || has_raw_part(path)
}

/// What the permissions of `extension_id` on `device` say about `path` (dialog choices aside).
fn decision(
    db: &VaultDb,
    host: &ExtensionHost,
    extension_id: Uuid,
    device: Uuid,
    path: &Path,
    access: Access,
) -> Result<Decision, BridgeError> {
    let mut grants = db
        .read_blocking(move |q| {
            candidates(q, extension_id, PermissionKind::Filesystem, device).map_err(Into::into)
        })
        .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))?;
    grants.extend(
        host.permissions
            .temporary(extension_id, PermissionKind::Filesystem),
    );
    let request = PermissionRequest {
        kind: PermissionKind::Filesystem,
        action: access.action(),
        target: RequestTarget::Path(path.to_path_buf()),
    };
    Ok(evaluate(&grants, &request, device))
}

/// What let a call reach its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allowed {
    /// A choice the user made in a dialog of the calling frame: it ends with that frame.
    Choice,
    /// A permission of the extension.
    Permission,
}

/// The real target of `raw` if the caller may reach it with `access` and `reach`; otherwise why
/// not: protected (1002), denied (1002), a question for the user (1004), or no free paths here
/// (8001).
pub fn authorize(
    ctx: &CallContext,
    raw: &str,
    access: Access,
    reach: Reach,
) -> Result<PathBuf, BridgeError> {
    authorize_by(ctx, raw, access, reach).map(|(path, _)| path)
}

/// [`authorize`], also telling what allowed it.
pub fn authorize_by(
    ctx: &CallContext,
    raw: &str,
    access: Access,
    reach: Reach,
) -> Result<(PathBuf, Allowed), BridgeError> {
    let environment = ctx.host.fs.environment()?;
    let path = resolve(raw)?;
    if touches_denied(&environment, &path, reach) {
        return Err(protected());
    }
    if ctx.host.fs.chosen(&ctx.session.frame, &path, access) {
        return Ok((path, Allowed::Choice));
    }
    if !environment.free_paths {
        return Err(BridgeError::not_available());
    }
    let action = match access {
        Access::Read => "read",
        Access::Write => "readWrite",
    };
    let (extension_id, device) = (ctx.session.extension_id, ctx.device);
    match decision(&ctx.db, &ctx.host, extension_id, device, &path, access)? {
        Decision::Allow => Ok((path, Allowed::Permission)),
        Decision::Deny => Err(BridgeError::new(
            ExtensionErrorCode::PermissionDenied,
            "permission denied",
        )),
        Decision::Prompt => Err(BridgeError::new(
            ExtensionErrorCode::PermissionPromptRequired,
            "permission required",
        )
        .with_details(json!({
            "resourceType": "filesystem",
            "action": action,
            "target": path.to_string_lossy(),
        }))),
    }
}

/// Ends the watches a permission allowed once no permission lets their extension read their
/// folder any more: a changed or revoked permission holds at once, also for a watch that is
/// already running (FR-020). A watch a dialog choice allowed ends with its frame (FR-048).
/// Blocking: reads the permissions; one that cannot be read ends the watch.
pub fn end_revoked_watches(db: &VaultDb, host: &ExtensionHost, device: Uuid) {
    host.fs.watches.end_unless(|extension_id, root| {
        matches!(
            decision(db, host, extension_id, device, root, Access::Read),
            Ok(Decision::Allow)
        )
    });
}

/// `{path, operation: read | write}` → `{status: granted | denied | ask}`; asks nothing.
pub fn check(
    ctx: &CallContext,
    params: &serde_json::Value,
) -> Result<serde_json::Value, BridgeError> {
    let invalid = || BridgeError::new(ExtensionErrorCode::Validation, "path and operation");
    let raw = params
        .get("path")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(invalid)?;
    let access = match params.get("operation").and_then(serde_json::Value::as_str) {
        Some("read") => Access::Read,
        Some("write") => Access::Write,
        _ => return Err(invalid()),
    };
    if ctx
        .host
        .fs
        .document(&ctx.session.frame, raw, access)
        .is_some()
    {
        return Ok(json!({ "status": "granted" }));
    }
    let status = match authorize(ctx, raw, access, Reach::Path) {
        Ok(_) => "granted",
        Err(error) if error.code == ExtensionErrorCode::PermissionPromptRequired => "ask",
        Err(error) if error.code == ExtensionErrorCode::Validation => return Err(error),
        Err(_) => "denied",
    };
    Ok(json!({ "status": status }))
}

#[cfg(all(test, not(any(target_os = "android", target_os = "ios"))))]
#[path = "fs_encrypted_tests.rs"]
mod fs_encrypted_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
#[path = "fs_tests.rs"]
mod tests;
#[cfg(all(test, not(any(target_os = "android", target_os = "ios"))))]
#[path = "watch_revoke_tests.rs"]
mod watch_revoke_tests;

//! Watching folders for an extension (spec 017, US9, FR-046, research R19 step 6): one watch per
//! (extension, `ruleId`), so an extension can only end or ask about its own. Every path of a
//! debounced batch reaches only the frames of that extension as `filesync:file-changed` with
//! `ruleId`, `changeType` and the path relative to the watched folder (flat, as the SDK reads it).
//! A watch ends with `unwatch`, when the last frame of its extension closes, or when no permission
//! allows its folder any more ([`super::end_revoked_watches`]). Android and iOS offer no watching
//! (FR-066).

// Without a watcher (Android, iOS) only the "not available" answers remain.
#![cfg_attr(
    any(target_os = "android", target_os = "ios"),
    allow(unused_imports, dead_code)
)]

use serde_json::{json, Value};

use super::{authorize_by, end_revoked_watches, Access, Allowed, Reach};
use crate::extensions::bridge::dispatch::CallContext;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};

pub const MODULE: &str = module_path!();

/// The event type of a changed path.
pub const FILE_CHANGED: &str = "filesync:file-changed";

/// Longest `ruleId` an extension may name.
const MAX_RULE_ID: usize = 256;

fn rule_id(params: &Value) -> Result<String, BridgeError> {
    params
        .get("ruleId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= MAX_RULE_ID)
        .map(str::to_owned)
        .ok_or_else(|| BridgeError::new(ExtensionErrorCode::Validation, "ruleId is missing"))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

    use serde_json::json;
    use uuid::Uuid;

    use super::FILE_CHANGED;
    use crate::extensions::bridge::dispatch::Emit;
    use crate::extensions::bridge::events::emit_to_frames;
    use crate::extensions::host::ExtensionHost;
    use crate::files::local::watch::{watch_folder, FolderWatch};

    /// A running watch, its folder and, when only a dialog choice allowed it, the frame of that
    /// choice. Dropping it ends the watch (`files::local::watch`).
    struct Watch {
        _watch: FolderWatch,
        root: PathBuf,
        frame: Option<String>,
    }

    /// The running watches, by (extension, `ruleId`).
    #[derive(Default)]
    pub struct Watches(Mutex<HashMap<(Uuid, String), Watch>>);

    impl Watches {
        fn map(&self) -> MutexGuard<'_, HashMap<(Uuid, String), Watch>> {
            self.0.lock().unwrap_or_else(PoisonError::into_inner)
        }

        /// Watches `root`; with `frame`, the watch ends when that frame closes. Links below
        /// `root` are not followed: their targets were never checked.
        pub fn start(
            &self,
            host: Weak<ExtensionHost>,
            emitter: Arc<dyn Emit>,
            extension_id: Uuid,
            rule_id: String,
            root: PathBuf,
            frame: Option<String>,
        ) -> Result<(), String> {
            let rule = rule_id.clone();
            let base = root.clone();
            let watch = watch_folder(&root, true, move |changes| {
                let Some(host) = host.upgrade() else {
                    return;
                };
                for change in changes {
                    let Some(path) = relative(&base, &change.path) else {
                        continue;
                    };
                    emit_to_frames(
                        emitter.as_ref(),
                        &host,
                        extension_id,
                        FILE_CHANGED,
                        &json!({
                            "ruleId": rule,
                            "changeType": change.kind.as_str(),
                            "path": path,
                        }),
                    );
                }
            })?;
            self.map().insert(
                (extension_id, rule_id),
                Watch {
                    _watch: watch,
                    root,
                    frame,
                },
            );
            Ok(())
        }

        /// Ends the watches only a choice of `frame` allowed.
        pub fn end_frame(&self, frame: &str) {
            self.map()
                .retain(|_, watch| watch.frame.as_deref() != Some(frame));
        }

        pub fn stop(&self, extension_id: Uuid, rule_id: &str) -> bool {
            self.map()
                .remove(&(extension_id, rule_id.to_owned()))
                .is_some()
        }

        pub fn running(&self, extension_id: Uuid, rule_id: &str) -> bool {
            self.map().contains_key(&(extension_id, rule_id.to_owned()))
        }

        pub fn end_all(&self, extension_id: Uuid) {
            self.map().retain(|(owner, _), _| *owner != extension_id);
        }

        /// Ends every watch a permission allowed whose folder `allowed` no longer grants its
        /// extension; a watch a dialog choice allowed ends with its frame instead.
        pub fn end_unless(&self, allowed: impl Fn(Uuid, &Path) -> bool) {
            self.map().retain(|(extension_id, _), watch| {
                watch.frame.is_some() || allowed(*extension_id, &watch.root)
            });
        }

        /// Ends every watch of a folder at or below `prefix`, also one a dialog choice allowed,
        /// before a drive is released (spec 064 research R9); returns how many ended.
        pub fn end_under(&self, prefix: &Path) -> usize {
            let mut map = self.map();
            let before = map.len();
            map.retain(|_, watch| !watch.root.starts_with(prefix));
            before - map.len()
        }
    }

    /// `path` below `root` as the SDK reports it; a path outside the watched folder, or inside the
    /// raw directory of an encrypted folder (spec 048 FR-051), is not reported.
    fn relative(root: &Path, path: &Path) -> Option<String> {
        path.strip_prefix(root)
            .ok()
            .filter(|rest| !crate::files::encrypted::device::has_raw_part(rest))
            .map(|rest| rest.to_string_lossy().into_owned())
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::Watches;

/// No watching on this platform.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[derive(Default)]
pub struct Watches;

#[cfg(any(target_os = "android", target_os = "ios"))]
impl Watches {
    pub fn end_frame(&self, _frame: &str) {}
    pub fn end_all(&self, _extension_id: uuid::Uuid) {}
    pub fn end_unless(&self, _allowed: impl Fn(uuid::Uuid, &std::path::Path) -> bool) {}
    pub fn end_under(&self, _prefix: &std::path::Path) -> usize {
        0
    }
}

/// `{ruleId, path}`: watches the folder and everything below it; a watch with the same `ruleId`
/// is replaced.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn watch(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    BridgeError::unless(crate::platform::capabilities().folder_watch)?;
    let rule = rule_id(params)?;
    let raw = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| BridgeError::new(ExtensionErrorCode::Validation, "path is missing"))?;
    let (root, allowed) = authorize_by(ctx, raw, Access::Read, Reach::Tree)?;
    // A choice holds only while its frame is open (FR-048), and so does what it allowed.
    let frame = (allowed == Allowed::Choice).then(|| ctx.session.frame.clone());
    ctx.host
        .fs
        .watches
        .start(
            std::sync::Arc::downgrade(&ctx.host),
            std::sync::Arc::clone(&ctx.emitter),
            ctx.session.extension_id,
            rule.clone(),
            root,
            frame,
        )
        .map_err(|e| {
            BridgeError::new(ExtensionErrorCode::Filesystem, format!("watch failed: {e}"))
        })?;
    // A permission revoked between the check above and the start ended the running watches
    // before this one was among them: check again now that it is (FR-020).
    if allowed == Allowed::Permission {
        end_revoked_watches(&ctx.db, &ctx.host, ctx.device);
        if !ctx.host.fs.watches.running(ctx.session.extension_id, &rule) {
            return Err(BridgeError::new(
                ExtensionErrorCode::PermissionDenied,
                "permission denied",
            ));
        }
    }
    Ok(Value::Null)
}

/// `{ruleId}`: ends the caller's own watch.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn unwatch(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let rule = rule_id(params)?;
    ctx.host.fs.watches.stop(ctx.session.extension_id, &rule);
    Ok(Value::Null)
}

/// `{ruleId}` → whether the caller watches with it.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn is_watching(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let rule = rule_id(params)?;
    Ok(json!(ctx
        .host
        .fs
        .watches
        .running(ctx.session.extension_id, &rule)))
}

// Mobile builds have no file watcher to compile (notify is a desktop-only dependency); the
// capability table says `folder_watch: false` there, so these answer "not available".
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn watch(_ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    Err(BridgeError::not_available())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn unwatch(_ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    Err(BridgeError::not_available())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn is_watching(_ctx: &CallContext, _params: &Value) -> Result<Value, BridgeError> {
    Err(BridgeError::not_available())
}

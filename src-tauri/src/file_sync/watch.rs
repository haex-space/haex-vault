//! Watching the local place of a rule (research R8): the watcher of the file browser, recursive,
//! links not followed; the control loop waits until the place is quiet. Not on iOS. A file rule watches
//! its one file (spec 074 research R7).

use std::sync::Arc;

use super::model::SyncRulePlace;
use crate::files::SourceRef;

/// Watches `place` when it is a folder of this device; `notify` hears each batch of changes.
pub fn watch_place(
    place: &SyncRulePlace,
    notify: Arc<dyn Fn() + Send + Sync>,
) -> Option<Box<dyn Send>> {
    if place.location != SourceRef::Device {
        return None;
    }
    #[cfg(not(target_os = "ios"))]
    {
        let root = std::path::Path::new(&place.path);
        match crate::files::local::watch::watch_folder(root, true, move |_| notify()) {
            Ok(watch) => Some(Box::new(watch)),
            Err(error) => {
                log::warn!("file_sync: a place cannot be watched: {error}");
                None
            }
        }
    }
    #[cfg(target_os = "ios")]
    {
        let _ = notify;
        None
    }
}

/// Watches the one file `name` in `parent` when it lies on this device (spec 074 research R7): the
/// parent folder, not recursively, and `notify` hears only a batch with a change of that file. A write in
/// place and a new file renamed over it both report the file's own name, so an editor that replaces the
/// file is heard, and a sibling is not.
pub fn watch_file(
    location: &SourceRef,
    parent: &str,
    name: &str,
    notify: Arc<dyn Fn() + Send + Sync>,
) -> Option<Box<dyn Send>> {
    if *location != SourceRef::Device {
        return None;
    }
    #[cfg(not(target_os = "ios"))]
    {
        let name = std::ffi::OsString::from(name);
        match crate::files::local::watch::watch_folder(
            std::path::Path::new(parent),
            false,
            move |changes| {
                // Reads and closes of the file (`Other`) are no change: a run reads the file itself.
                if changes.iter().any(|change| {
                    change.kind != crate::files::local::watch::ChangeKind::Other
                        && change.path.file_name() == Some(name.as_os_str())
                }) {
                    notify();
                }
            },
        ) {
            Ok(watch) => Some(Box::new(watch)),
            Err(error) => {
                log::warn!("file_sync: a file cannot be watched: {error}");
                None
            }
        }
    }
    #[cfg(target_os = "ios")]
    {
        let _ = (parent, name, notify);
        None
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;

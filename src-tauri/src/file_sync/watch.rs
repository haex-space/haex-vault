//! Watching the local place of a rule (research R8): the watcher of the file browser, recursive,
//! links not followed; the control loop waits until the place is quiet. Not on iOS.

use std::sync::Arc;

use super::model::SyncRulePlace;
use crate::files::SourceRef;

/// Watches `place` when it is a folder of this device; `notify` hears each batch of changes.
pub fn watch_place(
    place: &SyncRulePlace,
    notify: Arc<dyn Fn() + Send + Sync>,
) -> Option<Box<dyn Send>> {
    if place.source != SourceRef::Device {
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

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;

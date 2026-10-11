//! What stays behind in an encrypted folder and goes later (format "Aufräumen", FR-016, research
//! R7): sidecars another version names as `base`, and content objects no sidecar points at (an
//! upload that never got its sidecar, a version whose deletion failed half way). Both go only when
//! the provider changed them more than [`KEEP_MS`] ago, so an upload of another device that has
//! not written its sidecar yet stays. A folder with a sidecar that does not open keeps its content
//! objects: that sidecar may point at one.
//!
//! The cleanup runs after a folder is loaded, in the background of the vault session; nothing of
//! it shows in the window, and a failure waits for the next load.

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use futures::StreamExt;
use tokio::time::Instant;

use super::format::Id;
use super::load::{LoadedFolder, AT_ONCE};
use super::tree::facts;
use crate::remote_storage::{Access, RemoteStore};

/// How long the provider must not have changed an object before the cleanup takes it.
pub const KEEP_MS: i64 = 24 * 60 * 60 * 1000;

/// What a cleanup deletes: object keys under the folder's prefix.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Leftovers {
    pub contents: Vec<Id>,
    pub sidecars: Vec<Id>,
}

impl Leftovers {
    pub fn is_empty(&self) -> bool {
        self.contents.is_empty() && self.sidecars.is_empty()
    }
}

/// What of `loaded` can go at `now_ms`.
pub fn leftovers(loaded: &LoadedFolder, now_ms: i64) -> Leftovers {
    let old = |id: &Id| {
        loaded
            .times
            .get(id)
            .is_some_and(|time| now_ms.saturating_sub(*time) > KEEP_MS)
    };
    let mut by_entry: HashMap<Id, Vec<&super::cache::EntryRow>> = HashMap::new();
    for row in &loaded.versions {
        by_entry.entry(row.sidecar.entry).or_default().push(row);
    }
    let mut sidecars = Vec::new();
    for rows in by_entry.values() {
        let named: HashSet<Id> = rows.iter().filter_map(|row| row.sidecar.base).collect();
        sidecars.extend(
            rows.iter()
                .map(|row| row.sid)
                .filter(|sid| named.contains(sid) && old(sid)),
        );
    }
    let mut contents = Vec::new();
    if loaded.broken.is_empty() {
        let pointed: HashSet<Id> = loaded
            .versions
            .iter()
            .filter(|row| !sidecars.contains(&row.sid))
            .filter_map(|row| {
                let version = super::tree::Version {
                    sid: row.sid,
                    sidecar: row.sidecar.clone(),
                };
                facts(&version).map(|facts| facts.content)
            })
            .collect();
        contents.extend(
            loaded
                .contents
                .keys()
                .filter(|oid| !pointed.contains(*oid) && old(oid)),
        );
    }
    sidecars.sort_unstable();
    contents.sort_unstable();
    Leftovers { contents, sidecars }
}

/// Deletes `leftovers` of the folder `prefix`: the content objects, then the sidecars. A failed
/// delete stays for the next load.
pub async fn clean(store: &dyn RemoteStore, access: &Access, prefix: &str, leftovers: &Leftovers) {
    let key = |part: &str, id: &Id| format!("{prefix}{part}/{}", super::format::encode_id(id));
    let contents = leftovers.contents.iter().map(|oid| key("c", oid)).collect();
    let sidecars = leftovers.sidecars.iter().map(|sid| key("m", sid)).collect();
    for batch in [contents, sidecars] {
        futures::stream::iter::<Vec<String>>(batch)
            .for_each_concurrent(AT_ONCE, |key| async move {
                let deadline = Instant::now() + std::time::Duration::from_secs(30);
                let _ = store.delete(access, &key, deadline).await;
            })
            .await;
    }
}

/// Milliseconds since 1970 now, as the provider counts its modification times.
pub fn now_ms() -> i64 {
    crate::passwords::clock::unix_millis(SystemTime::now())
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;

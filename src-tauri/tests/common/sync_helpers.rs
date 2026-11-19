//! Helpers the sync integration tests share: the running service of an
//! instance, polling until something holds, and the effective device list.
//! Included with `#[path]` next to `common/sync_fixture.rs`.

#![cfg(target_os = "linux")]
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use tauri::Manager;

use haex_lib::storage::query;
use haex_lib::sync::device_list;
use haex_lib::sync::registry::{SyncRegistry, SyncRuntime};

use crate::sync_fixture::Instance;

/// The device's sync service, once it has bound (it starts in the
/// background).
pub async fn runtime_of(device: &Instance) -> Arc<SyncRuntime> {
    until("the sync service to come up", || {
        device.app.state::<Arc<SyncRegistry>>().get()
    })
    .await
}

pub async fn until<T>(what: &str, mut check: impl FnMut() -> Option<T>) -> T {
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            if let Some(found) = check() {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
}

pub fn listed(main: &Instance) -> Vec<([u8; 32], bool)> {
    query::read(&main.database(), |r| {
        let valid = device_list::valid_lists(&device_list::load_all(r)?, &main.vault);
        Ok(device_list::effective(&valid)
            .map(|s| {
                s.list
                    .devices
                    .iter()
                    .map(|d| (d.device_pubkey, d.role == device_list::Role::Main))
                    .collect()
            })
            .unwrap_or_default())
    })
    .expect("read list")
}

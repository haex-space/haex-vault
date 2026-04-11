//! Async wrappers for synchronous MLS operations.
//!
//! MLS operations (crypto + SQLite) are CPU-bound and must not run on
//! the Tokio async runtime thread — doing so starves QUIC keep-alive
//! PINGs and causes iroh path idle timeouts (6.5s).

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::manager::MlsManager;
use super::types::{MlsCommitBundle, MlsEpochKey, MlsGroupInfo};

/// Run a synchronous MLS operation on a blocking thread.
async fn run_blocking<F, T>(conn: Arc<Mutex<Option<Connection>>>, f: F) -> Result<T, String>
where
    F: FnOnce(&MlsManager) -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let manager = MlsManager::new(conn);
        f(&manager)
    })
    .await
    .map_err(|e| format!("MLS task panicked: {e}"))?
}

pub async fn add_member(
    conn: Arc<Mutex<Option<Connection>>>,
    space_id: String,
    key_package: Vec<u8>,
) -> Result<MlsCommitBundle, String> {
    run_blocking(conn, move |mgr| mgr.add_member(&space_id, &key_package)).await
}

pub async fn get_group_info(
    conn: Arc<Mutex<Option<Connection>>>,
    space_id: String,
) -> Result<Vec<u8>, String> {
    run_blocking(conn, move |mgr| mgr.get_group_info(&space_id)).await
}

pub async fn process_message(
    conn: Arc<Mutex<Option<Connection>>>,
    space_id: String,
    message: Vec<u8>,
) -> Result<Vec<u8>, String> {
    run_blocking(conn, move |mgr| mgr.process_message(&space_id, &message)).await
}

pub async fn join_by_external_commit(
    conn: Arc<Mutex<Option<Connection>>>,
    space_id: String,
    group_info_bytes: Vec<u8>,
) -> Result<(Vec<u8>, MlsEpochKey), String> {
    run_blocking(conn, move |mgr| {
        mgr.join_by_external_commit(&space_id, &group_info_bytes)
    })
    .await
}

pub async fn generate_key_packages(
    conn: Arc<Mutex<Option<Connection>>>,
    count: u32,
) -> Result<Vec<Vec<u8>>, String> {
    run_blocking(conn, move |mgr| mgr.generate_key_packages(count)).await
}

pub async fn process_welcome(
    conn: Arc<Mutex<Option<Connection>>>,
    space_id: String,
    welcome_bytes: Vec<u8>,
) -> Result<MlsGroupInfo, String> {
    run_blocking(conn, move |mgr| {
        mgr.process_welcome(&space_id, &welcome_bytes)
    })
    .await
}

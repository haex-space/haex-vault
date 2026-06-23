use super::conflict::make_conflict_path;
use super::execute::execute_sync;
use crate::database::DbConnection;
use crate::file_sync::local_provider::LocalProvider;
use crate::file_sync::provider::{ReadFileResult, SyncProvider, SyncProviderError};
use crate::file_sync::types::FileState;
use crate::peer_storage::resume::PartialState;
use async_trait::async_trait;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[test]
fn conflict_path_with_extension() {
    let result = make_conflict_path("docs/report.pdf", 1700000000);
    assert_eq!(result, "docs/report.conflict.1700000000.pdf");
}

#[test]
fn conflict_path_without_extension() {
    let result = make_conflict_path("Makefile", 1700000000);
    assert_eq!(result, "Makefile.conflict.1700000000");
}

#[test]
fn conflict_path_root_level() {
    let result = make_conflict_path("file.txt", 1700000000);
    assert_eq!(result, "file.conflict.1700000000.txt");
}

// ---------------------------------------------------------------------
// Resume integration: engine routes downloads to the final destination
// when the target is local, and clears the partial sidecar on a
// manifest-hash mismatch so stale bytes do not survive into the next
// sync cycle.
// ---------------------------------------------------------------------

/// In-memory DB with the minimum tables `execute_sync` touches:
/// `haex_sync_state_no_sync` plus the CRDT bookkeeping tables that
/// `core::execute` flips `triggers_enabled` against. No HLC service is
/// installed — the `_no_sync` suffix on the target table means
/// `core::execute` skips the CRDT transformer pipeline.
fn test_db_with_sync_state() -> DbConnection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    conn.execute_batch(
        "CREATE TABLE haex_crdt_configs_no_sync (
            key TEXT PRIMARY KEY,
            type TEXT NOT NULL,
            value TEXT NOT NULL
        );
        CREATE TABLE haex_crdt_dirty_tables_no_sync (
            table_name TEXT PRIMARY KEY,
            last_modified TEXT
        );
        CREATE TABLE haex_sync_state_no_sync (
            id TEXT PRIMARY KEY NOT NULL,
            rule_id TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            modified_at INTEGER NOT NULL,
            synced_at TEXT NOT NULL,
            deleted INTEGER DEFAULT 0 NOT NULL,
            hash TEXT
        );
        CREATE UNIQUE INDEX haex_sync_state_rule_path_unique
            ON haex_sync_state_no_sync (rule_id, relative_path);",
    )
    .expect("schema setup");
    DbConnection(Arc::new(Mutex::new(Some(conn))))
}

/// A source provider whose manifest advertises a single file and
/// whose `read_file_to_path` records the output path passed to it so
/// the test can assert local-target downloads bypass the tempfile
/// staging path. The provider writes `payload` to that path. When
/// `report_hash` is `Some`, the returned `ReadFileResult.hash` is
/// that value — a different value than the manifest hash drives the
/// hash-mismatch branch.
struct RecordingSource {
    relative_path: String,
    payload: Vec<u8>,
    manifest_hash: String,
    report_hash: Option<String>,
    last_output_path: Arc<Mutex<Option<PathBuf>>>,
}

#[async_trait]
impl SyncProvider for RecordingSource {
    fn display_name(&self) -> String {
        "recording-source".to_string()
    }

    async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError> {
        Ok(vec![FileState {
            relative_path: self.relative_path.clone(),
            size: self.payload.len() as u64,
            modified_at: 1_700_000_000,
            is_directory: false,
            hash: Some(self.manifest_hash.clone()),
            chunk_size: None,
            chunk_hashes: None,
        }])
    }

    async fn read_file(&self, _relative_path: &str) -> Result<Vec<u8>, SyncProviderError> {
        Ok(self.payload.clone())
    }

    async fn read_file_to_path(
        &self,
        _relative_path: &str,
        output_path: &Path,
        _expected_chunks: Option<crate::file_sync::hashing::ChunkedHash>,
        on_progress: Arc<dyn Fn(u64, u64) + Send + Sync>,
    ) -> Result<ReadFileResult, SyncProviderError> {
        *self.last_output_path.lock().unwrap() = Some(output_path.to_path_buf());
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SyncProviderError::Io)?;
        }
        tokio::fs::write(output_path, &self.payload)
            .await
            .map_err(SyncProviderError::Io)?;
        let n = self.payload.len() as u64;
        on_progress(n, n);
        Ok(ReadFileResult {
            bytes: n,
            hash: self.report_hash.clone(),
        })
    }

    async fn write_file(
        &self,
        _relative_path: &str,
        _data: &[u8],
    ) -> Result<(), SyncProviderError> {
        unreachable!("RecordingSource is read-only")
    }

    async fn delete_file(
        &self,
        _relative_path: &str,
        _to_trash: bool,
    ) -> Result<(), SyncProviderError> {
        unreachable!("RecordingSource is read-only")
    }

    async fn create_directory(&self, _relative_path: &str) -> Result<(), SyncProviderError> {
        Ok(())
    }
}

/// Engine routes a download whose target is a `LocalProvider` directly
/// into `<base>/<relative_path>` — no tempfile. This is what lets the
/// resume sidecar (`<dest>.haex-partial.meta`) survive engine retries.
#[tokio::test]
async fn local_target_download_uses_final_destination_path() {
    let target_dir = tempfile::TempDir::new().unwrap();
    let target = Arc::new(LocalProvider::new(target_dir.path().to_path_buf()).unwrap());
    let expected_final = target_dir.path().join("note.txt");
    let recorded = Arc::new(Mutex::new(None));
    let source = Arc::new(RecordingSource {
        relative_path: "note.txt".to_string(),
        payload: b"hello local target".to_vec(),
        manifest_hash: "abc".to_string(),
        report_hash: Some("abc".to_string()),
        last_output_path: recorded.clone(),
    });
    let db = test_db_with_sync_state();

    let result = execute_sync(
        source,
        target,
        crate::file_sync::types::SyncDirection::OneWay,
        crate::file_sync::types::DeleteMode::Ignore,
        "rule-local-target",
        &db,
        None,
        None,
    )
    .await
    .expect("execute_sync");

    assert!(
        result.errors.is_empty(),
        "expected no errors, got {:?}",
        result.errors
    );
    assert_eq!(result.files_downloaded, 1);
    let recorded_path = recorded.lock().unwrap().clone().expect("source called");
    assert_eq!(
        recorded_path, expected_final,
        "engine should have passed the final destination, not a tempfile"
    );
    assert!(expected_final.exists(), "destination file must exist");
}

/// On a manifest-hash mismatch (source drift mid-flight) the engine
/// clears the resume sidecar at the final destination — otherwise the
/// next sync cycle would resume from stale bytes that no longer
/// match the manifest.
#[tokio::test]
async fn manifest_mismatch_clears_partial_sidecar() {
    let target_dir = tempfile::TempDir::new().unwrap();
    let target = Arc::new(LocalProvider::new(target_dir.path().to_path_buf()).unwrap());
    let final_path = target_dir.path().join("drifted.bin");

    // Pre-seed a partial + sidecar — they simulate the state left by a
    // prior aborted transfer. After the engine sees the manifest
    // mismatch it must wipe both.
    let partial_path = PartialState::partial_path(&final_path);
    tokio::fs::write(&partial_path, b"old partial bytes")
        .await
        .unwrap();
    let state = PartialState {
        file_hash: "stale-hash".to_string(),
        chunk_size: 1024,
        completed: vec![true, false],
    };
    state.save(&final_path).await.unwrap();
    let meta_path = {
        let mut p = final_path.clone().into_os_string();
        p.push(".haex-partial.meta");
        PathBuf::from(p)
    };
    assert!(meta_path.exists(), "precondition: sidecar exists");

    let source = Arc::new(RecordingSource {
        relative_path: "drifted.bin".to_string(),
        payload: b"fresh bytes after manifest changed".to_vec(),
        manifest_hash: "manifest-claims-this".to_string(),
        // Source reports a different hash than the manifest — this is
        // the in-flight-drift branch the engine must handle.
        report_hash: Some("source-actually-sent-this".to_string()),
        last_output_path: Arc::new(Mutex::new(None)),
    });
    let db = test_db_with_sync_state();

    let result = execute_sync(
        source,
        target,
        crate::file_sync::types::SyncDirection::OneWay,
        crate::file_sync::types::DeleteMode::Ignore,
        "rule-mismatch",
        &db,
        None,
        None,
    )
    .await
    .expect("execute_sync");

    assert_eq!(
        result.files_downloaded, 0,
        "mismatch must not commit a download"
    );
    assert!(
        result.errors.iter().any(|e| e.contains("hash mismatch")),
        "expected hash-mismatch error, got {:?}",
        result.errors
    );
    assert!(
        !meta_path.exists(),
        "engine should have cleared the partial sidecar after manifest mismatch"
    );
    assert!(
        !partial_path.exists(),
        "engine should have cleared the partial bytes after manifest mismatch"
    );
}

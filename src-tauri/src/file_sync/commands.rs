//! Tauri commands for file sync engine control
//!
//! Bridges the frontend to the sync engine by providing commands to start/stop
//! sync rules, trigger immediate syncs, and query status.

use std::collections::HashMap;
use std::time::Duration;

use tauri::State;
use tokio_util::sync::CancellationToken;

use crate::AppState;

use std::sync::Arc;

use super::cloud_provider::CloudProvider;
use super::engine::{execute_sync, run_sync_loop, SyncEngineError};
use super::local_provider::LocalProvider;
use super::peer_provider::PeerProvider;
use super::provider::SyncProvider;
use super::types::{DeleteMode, SyncDirection, SyncResult};

// ---------------------------------------------------------------------------
// SyncManager
// ---------------------------------------------------------------------------

/// Manages active sync loops, keyed by rule ID.
pub struct SyncManager {
    /// Active sync loops: rule_id -> (cancellation token, trigger sender)
    active_rules: HashMap<String, (CancellationToken, tokio::sync::mpsc::Sender<()>)>,
}

impl SyncManager {
    pub fn new() -> Self {
        Self {
            active_rules: HashMap::new(),
        }
    }

    pub fn is_running(&self, rule_id: &str) -> bool {
        self.active_rules.contains_key(rule_id)
    }

    pub fn stop(&mut self, rule_id: &str) {
        if let Some((token, _)) = self.active_rules.remove(rule_id) {
            token.cancel();
        }
    }

    pub fn stop_all(&mut self) {
        for (_, (token, _)) in self.active_rules.drain() {
            token.cancel();
        }
    }

    pub fn register(
        &mut self,
        rule_id: String,
        token: CancellationToken,
        trigger_sender: tokio::sync::mpsc::Sender<()>,
    ) {
        self.active_rules.insert(rule_id, (token, trigger_sender));
    }

    pub fn running_rule_ids(&self) -> Vec<String> {
        self.active_rules.keys().cloned().collect()
    }

    /// Trigger an immediate sync for a running rule.
    pub async fn trigger(&self, rule_id: &str) {
        if let Some((_, sender)) = self.active_rules.get(rule_id) {
            let _ = sender.send(()).await;
        }
    }
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum FileSyncCommandError {
    #[error("Invalid config: {0}")]
    InvalidConfig(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
    #[error("Engine error: {0}")]
    EngineError(#[from] SyncEngineError),
    #[error("Not running: {0}")]
    NotRunning(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl serde::Serialize for FileSyncCommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncRuleStatus {
    pub rule_id: String,
    pub running: bool,
}

// ---------------------------------------------------------------------------
// Provider factory
// ---------------------------------------------------------------------------

/// Create a SyncProvider from type string and config JSON.
///
/// `is_target` controls whether missing containers (e.g. S3 buckets) get
/// auto-provisioned: only the sync target should auto-create its container —
/// a missing *source* bucket is almost always a misconfiguration and should
/// fail fast instead of being silently created.
async fn create_provider(
    provider_type: &str,
    config: &serde_json::Value,
    state: &AppState,
    is_target: bool,
) -> Result<Arc<dyn SyncProvider>, FileSyncCommandError> {
    match provider_type {
        "local" => {
            let path = config
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    FileSyncCommandError::InvalidConfig(
                        "local provider requires 'path'".into(),
                    )
                })?;
            let provider = LocalProvider::new(std::path::PathBuf::from(path))
                .map_err(|e| FileSyncCommandError::ProviderError(e.to_string()))?;
            Ok(Arc::new(provider))
        }
        "peer" => {
            let endpoint_id_str = config
                .get("endpointId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    FileSyncCommandError::InvalidConfig(
                        "peer provider requires 'endpointId'".into(),
                    )
                })?;
            let endpoint_id: iroh::EndpointId = endpoint_id_str.parse().map_err(|e| {
                FileSyncCommandError::InvalidConfig(format!("Invalid endpointId: {e}"))
            })?;
            // Look up the live relay URL from the CRDT — the value stored in
            // the sync rule config may be stale if the peer restarted.
            let relay_url = {
                let sql = "SELECT relay_url FROM haex_space_devices \
                           WHERE endpoint_id = ?1 LIMIT 1"
                    .to_string();
                let params = vec![serde_json::Value::String(endpoint_id_str.to_string())];
                crate::database::core::select_with_crdt(sql, params, &state.db)
                    .ok()
                    .and_then(|rows| rows.into_iter().next())
                    .and_then(|row| row.get(0).and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .and_then(|s| s.parse::<iroh::RelayUrl>().ok())
                    .or_else(|| {
                        // Fallback to config value
                        config
                            .get("relayUrl")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse::<iroh::RelayUrl>().ok())
                    })
            };
            let base_path = config
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("/")
                .to_string();
            let ucan_token = config
                .get("ucanToken")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    FileSyncCommandError::InvalidConfig(
                        "peer provider requires 'ucanToken'".into(),
                    )
                })?
                .to_string();

            let endpoint = state.peer_storage.clone();
            let provider = PeerProvider::new(endpoint, endpoint_id, relay_url, base_path, ucan_token);
            Ok(Arc::new(provider))
        }
        "cloud" => {
            let backend_id = config
                .get("backendId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    FileSyncCommandError::InvalidConfig(
                        "cloud provider requires 'backendId'".into(),
                    )
                })?;
            let prefix = config
                .get("prefix")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let bucket_override = config
                .get("bucket")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());

            let backend = crate::remote_storage::commands::get_backend_instance_from_db_with_overrides(
                &state.db,
                backend_id,
                bucket_override,
            )
            .await
            .map_err(|e| FileSyncCommandError::ProviderError(e.to_string()))?;

            // Auto-create the bucket only when this provider is the sync
            // target — a missing *source* bucket is almost always a typo or
            // stale config and should surface as an error instead.
            if is_target {
                backend
                    .ensure_container()
                    .await
                    .map_err(|e| FileSyncCommandError::ProviderError(e.to_string()))?;
            }

            let provider = CloudProvider::new(backend, prefix);
            Ok(Arc::new(provider))
        }
        _ => Err(FileSyncCommandError::InvalidConfig(format!(
            "Unknown provider type: {provider_type}"
        ))),
    }
}

/// Parse a direction string into `SyncDirection`.
fn parse_direction(direction: &str) -> Result<SyncDirection, FileSyncCommandError> {
    match direction {
        "one_way" => Ok(SyncDirection::OneWay),
        "two_way" => Ok(SyncDirection::TwoWay),
        _ => Err(FileSyncCommandError::InvalidConfig(format!(
            "Unknown direction: {direction}"
        ))),
    }
}

/// Parse a delete mode string into `DeleteMode`.
fn parse_delete_mode(delete_mode: &str) -> Result<DeleteMode, FileSyncCommandError> {
    match delete_mode {
        "trash" => Ok(DeleteMode::Trash),
        "permanent" => Ok(DeleteMode::Permanent),
        "ignore" => Ok(DeleteMode::Ignore),
        _ => Err(FileSyncCommandError::InvalidConfig(format!(
            "Unknown delete mode: {delete_mode}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Start syncing for a specific rule. Creates providers and spawns a periodic sync loop.
#[tauri::command(rename_all = "camelCase")]
pub async fn file_sync_start_rule(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    rule_id: String,
    source_type: String,
    source_config: serde_json::Value,
    target_type: String,
    target_config: serde_json::Value,
    direction: String,
    delete_mode: String,
    interval_seconds: u64,
) -> Result<(), FileSyncCommandError> {
    eprintln!("[FileSync] Starting rule {rule_id}: {source_type} → {target_type}, interval={interval_seconds}s");

    let dir = parse_direction(&direction)?;
    let del = parse_delete_mode(&delete_mode)?;

    // Stop any existing loop for this rule
    {
        let mut manager = state.sync_manager.lock().await;
        manager.stop(&rule_id);
    }

    let source = create_provider(&source_type, &source_config, &state, false).await
        .inspect_err(|e| eprintln!("[FileSync] Failed to create source provider: {e}"))?;
    let target = create_provider(&target_type, &target_config, &state, true).await
        .inspect_err(|e| eprintln!("[FileSync] Failed to create target provider: {e}"))?;

    let cancel = CancellationToken::new();
    let (trigger_sender, trigger_receiver) = tokio::sync::mpsc::channel::<()>(16);
    let db = crate::database::DbConnection(state.db.0.clone());
    let rule_id_clone = rule_id.clone();

    // Register before spawning so status queries see it immediately
    {
        let mut manager = state.sync_manager.lock().await;
        manager.register(rule_id.clone(), cancel.clone(), trigger_sender.clone());
    }

    // Start file watcher for local providers — directly triggers sync loop
    if target_type == "local" {
        if let Some(path) = target_config.get("path").and_then(|v| v.as_str()) {
            let _ = state
                .file_watcher
                .watch(app.clone(), rule_id.clone(), path.to_string(), Some(trigger_sender.clone()));
        }
    }
    if source_type == "local" {
        if let Some(path) = source_config.get("path").and_then(|v| v.as_str()) {
            let watcher_key = format!("{}_source", rule_id);
            let _ = state
                .file_watcher
                .watch(app.clone(), watcher_key, path.to_string(), Some(trigger_sender.clone()));
        }
    }

    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        run_sync_loop(
            source,
            target,
            dir,
            del,
            rule_id_clone,
            Duration::from_secs(interval_seconds),
            cancel,
            trigger_receiver,
            db,
            app_clone,
        )
        .await;
    });

    Ok(())
}

/// Stop syncing for a specific rule.
#[tauri::command(rename_all = "camelCase")]
pub async fn file_sync_stop_rule(
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<(), FileSyncCommandError> {
    let mut manager = state.sync_manager.lock().await;
    if !manager.is_running(&rule_id) {
        return Err(FileSyncCommandError::NotRunning(rule_id));
    }
    manager.stop(&rule_id);

    // Stop file watchers for this rule
    let _ = state.file_watcher.unwatch(&rule_id);
    let _ = state.file_watcher.unwatch(&format!("{}_source", rule_id));

    Ok(())
}

/// Trigger an immediate one-shot sync for a rule.
#[tauri::command(rename_all = "camelCase")]
pub async fn file_sync_trigger_now(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    rule_id: String,
    source_type: String,
    source_config: serde_json::Value,
    target_type: String,
    target_config: serde_json::Value,
    direction: String,
    delete_mode: String,
) -> Result<SyncResult, FileSyncCommandError> {
    let dir = parse_direction(&direction)?;
    let del = parse_delete_mode(&delete_mode)?;

    let source = create_provider(&source_type, &source_config, &state, false).await?;
    let target = create_provider(&target_type, &target_config, &state, true).await?;

    let result = execute_sync(
        source,
        target,
        dir,
        del,
        &rule_id,
        &state.db,
        Some(app),
        None,
    )
    .await?;

    Ok(result)
}

/// Get status of all active sync rules.
#[tauri::command]
pub async fn file_sync_status(
    state: State<'_, AppState>,
) -> Result<Vec<SyncRuleStatus>, FileSyncCommandError> {
    let manager = state.sync_manager.lock().await;
    let statuses = manager
        .running_rule_ids()
        .into_iter()
        .map(|rule_id| SyncRuleStatus {
            rule_id,
            running: true,
        })
        .collect();
    Ok(statuses)
}

/// Stop all active sync loops.
#[tauri::command]
pub async fn file_sync_stop_all(
    state: State<'_, AppState>,
) -> Result<(), FileSyncCommandError> {
    let mut manager = state.sync_manager.lock().await;
    manager.stop_all();

    // Stop all file watchers
    let _ = state.file_watcher.unwatch_all();

    Ok(())
}

/// Trigger an immediate sync for a running rule (e.g. from file watcher events).
#[tauri::command(rename_all = "camelCase")]
pub async fn file_sync_trigger_by_watcher(
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<(), FileSyncCommandError> {
    let manager = state.sync_manager.lock().await;
    manager.trigger(&rule_id).await;
    Ok(())
}

#[derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncLogRow {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    /// JSON payload `{ summary, raw? }` — kept as a string so the frontend
    /// can parse it without an extra Tauri schema dance.
    pub message: String,
    pub device_id: String,
}

/// Load persisted sync log entries for a rule.
///
/// Reads from the CRDT-synced `haex_logs` table, filtered by
/// `source = 'file-sync'` and the rule ID stored in `metadata.ruleId`. The
/// rule ID lives in `metadata` (not `extension_id`) because `extension_id`
/// has a FK on `haex_extensions(id)` and sync rules are not extensions.
/// When `all_devices` is false (default), entries are additionally filtered
/// to the current device's logs only.
#[tauri::command(rename_all = "camelCase")]
pub async fn file_sync_get_log(
    state: State<'_, AppState>,
    rule_id: String,
    limit: Option<u32>,
    all_devices: Option<bool>,
) -> Result<Vec<SyncLogRow>, FileSyncCommandError> {
    use serde_json::Value as JsonValue;

    let lim = limit.unwrap_or(50).min(500) as i64;
    let all = all_devices.unwrap_or(false);

    let device_id = state
        .context
        .lock()
        .map(|ctx| ctx.device_id.clone())
        .unwrap_or_default();

    let table = crate::table_names::TABLE_LOGS;
    let (sql, params) = if all {
        (
            format!(
                "SELECT id, timestamp, level, message, device_id FROM {table} \
                 WHERE source = 'file-sync' AND json_extract(metadata, '$.ruleId') = ?1 \
                 ORDER BY timestamp DESC LIMIT ?2"
            ),
            vec![JsonValue::String(rule_id), JsonValue::Number(lim.into())],
        )
    } else {
        (
            format!(
                "SELECT id, timestamp, level, message, device_id FROM {table} \
                 WHERE source = 'file-sync' AND json_extract(metadata, '$.ruleId') = ?1 AND device_id = ?2 \
                 ORDER BY timestamp DESC LIMIT ?3"
            ),
            vec![
                JsonValue::String(rule_id),
                JsonValue::String(device_id),
                JsonValue::Number(lim.into()),
            ],
        )
    };

    // select_with_crdt automatically filters tombstoned rows so a previous
    // clear_log call stays cleared after a reload.
    let rows = crate::database::core::select_with_crdt(sql, params, &state.db)
        .map_err(|e| FileSyncCommandError::Internal(e.to_string()))?;

    fn opt_str(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        }
    }

    let result = rows
        .iter()
        .map(|row| SyncLogRow {
            id: opt_str(row.first().unwrap_or(&serde_json::Value::Null)),
            timestamp: opt_str(row.get(1).unwrap_or(&serde_json::Value::Null)),
            level: opt_str(row.get(2).unwrap_or(&serde_json::Value::Null)),
            message: opt_str(row.get(3).unwrap_or(&serde_json::Value::Null)),
            device_id: opt_str(row.get(4).unwrap_or(&serde_json::Value::Null)),
        })
        .collect();

    Ok(result)
}

/// Soft-delete all sync log entries for a rule via CRDT.
///
/// Uses `execute_with_crdt` so the tombstone propagates across devices — a
/// hard delete would re-sync from peers on the next pull.
#[tauri::command(rename_all = "camelCase")]
pub async fn file_sync_clear_log(
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<(), FileSyncCommandError> {
    use serde_json::Value as JsonValue;

    let hlc = state
        .hlc
        .lock()
        .map_err(|e| FileSyncCommandError::Internal(format!("HLC lock: {e}")))?;
    let table = crate::table_names::TABLE_LOGS;
    let sql = format!("DELETE FROM {table} WHERE source = 'file-sync' AND extension_id = ?1");
    crate::database::core::execute_with_crdt(
        sql,
        vec![JsonValue::String(rule_id)],
        &state.db,
        &hlc,
    )
    .map_err(|e| FileSyncCommandError::Internal(e.to_string()))?;
    Ok(())
}

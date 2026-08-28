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

use crate::database::DbConnection;

use super::cloud_provider::CloudProvider;
use super::crypto::provider::EncryptingSyncProvider;
use super::crypto::space_provider::SpaceContentSyncProvider;
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
    /// Active sync loops: rule_id -> (cancellation token, trigger sender, join handle)
    ///
    /// The `JoinHandle` is retained so `stop`/`stop_all` can await the spawned
    /// `run_sync_loop` future. If the future panicked, the awaited result is
    /// surfaced via `log::error!` with the rule id so the failure is no longer
    /// silent.
    active_rules: HashMap<
        String,
        (
            CancellationToken,
            tokio::sync::mpsc::Sender<()>,
            tokio::task::JoinHandle<()>,
        ),
    >,
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

    /// Cancel a rule and take ownership of its JoinHandle so the caller can
    /// `await` it AFTER releasing the `SyncManager` lock. The split exists
    /// because `auto_disable_rule` re-enters the same lock via `deregister`
    /// — awaiting the handle under the lock would self-deadlock if that
    /// task is currently waiting on the lock.
    ///
    /// Caller pattern:
    ///
    /// ```ignore
    /// let handle = {
    ///     let mut manager = state.sync_manager.lock().await;
    ///     manager.take_stop(&rule_id)
    /// };
    /// if let Some(handle) = handle {
    ///     await_sync_handle(&rule_id, handle).await;
    /// }
    /// ```
    pub fn take_stop(&mut self, rule_id: &str) -> Option<tokio::task::JoinHandle<()>> {
        self.active_rules.remove(rule_id).map(|(token, _, handle)| {
            token.cancel();
            handle
        })
    }

    /// Remove a rule's registration without awaiting its JoinHandle. Used by
    /// in-task exit paths (the sync loop deregistering itself) to avoid the
    /// self-await deadlock that the awaiting `stop` would produce. The
    /// cancellation token is still cancelled — callers higher up will then
    /// observe `is_running == false` and the cancelled token; the spawned
    /// task is already exiting on its own.
    pub fn deregister(&mut self, rule_id: &str) {
        if let Some((token, _, _handle)) = self.active_rules.remove(rule_id) {
            token.cancel();
            // _handle dropped → task continues to completion detached. That's
            // fine because the only caller (`auto_disable_rule`) is itself
            // running inside that task and is about to return.
        }
    }

    /// Cancel all rules and drain their JoinHandles for the caller to await
    /// AFTER releasing the `SyncManager` lock. Same deadlock-avoidance
    /// rationale as [`Self::take_stop`].
    pub fn take_stop_all(&mut self) -> Vec<(String, tokio::task::JoinHandle<()>)> {
        self.active_rules
            .drain()
            .map(|(rule_id, (token, _, handle))| {
                token.cancel();
                (rule_id, handle)
            })
            .collect()
    }

    pub fn register(
        &mut self,
        rule_id: String,
        token: CancellationToken,
        trigger_sender: tokio::sync::mpsc::Sender<()>,
        handle: tokio::task::JoinHandle<()>,
    ) {
        self.active_rules
            .insert(rule_id, (token, trigger_sender, handle));
    }

    pub fn running_rule_ids(&self) -> Vec<String> {
        self.active_rules.keys().cloned().collect()
    }

    /// Trigger an immediate sync for a running rule.
    pub async fn trigger(&self, rule_id: &str) {
        if let Some((_, sender, _)) = self.active_rules.get(rule_id) {
            let _ = sender.send(()).await;
        }
    }
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Await a drained sync-loop JoinHandle and surface any abnormal termination.
/// Centralised so all `take_stop`/`take_stop_all` call sites format their
/// failure log identically.
async fn await_sync_handle(rule_id: &str, handle: tokio::task::JoinHandle<()>) {
    if let Err(join_err) = handle.await {
        eprintln!(
            "[FileSync] run_sync_loop task for rule {rule_id} terminated abnormally: {join_err}"
        );
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
    /// No `haex_s3_shared_access` row for this viewer against
    /// `(space_id, backend_id)`. Semantically distinct from
    /// [`Self::InvalidConfig`]: this viewer's capability to sync the space
    /// via that backend has been revoked (or was never granted), and the UI
    /// should surface a share-revoked / re-enrollment prompt rather than a
    /// generic config-invalid message. The `member_did` field carries the
    /// DID the lookup targeted so callers can log it out-of-band; the
    /// display message deliberately elides it to keep viewer DIDs out of
    /// user-facing error text.
    #[error("no active share for space={space_id} backend={backend_id}")]
    NotShared {
        space_id: String,
        backend_id: String,
        member_did: String,
    },
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

/// Look up which space (if any) a `haex_s3_backends` row is bound to via a
/// cross-user share. Mirrors the join `find_existing_share` in
/// `remote_storage::share_command` uses (`haex_shared_space_sync.row_pks[0]`
/// holds the backend id for `table_name = 'haex_s3_backends'` rows) plus the
/// authoritative provenance predicate `origin_type = 'shared_from_space'` on
/// the backend row itself — a mapping alone is not proof of a cross-user
/// share.
///
/// The schema does not prevent multiple `haex_shared_space_sync` rows from
/// pointing at the same backend, so the query counts distinct space bindings
/// and treats more than one as a hard error rather than silently picking one.
fn shared_backend_space_id(
    backend_id: &str,
    db: &crate::database::DbConnection,
) -> Result<Option<String>, FileSyncCommandError> {
    let sql = format!(
        "SELECT DISTINCT m.{col_space} \
         FROM {table_map} m \
         INNER JOIN {table_backends} b \
           ON b.{col_backend_id} = json_extract(m.{col_pks}, '$[0]') \
          AND b.{col_backend_origin} = 'shared_from_space' \
         WHERE m.{col_table} = ?1 \
           AND json_extract(m.{col_pks}, '$[0]') = ?2 \
         LIMIT 2",
        col_space = crate::table_names::COL_SHARED_SPACE_SYNC_SPACE_ID,
        table_map = crate::table_names::TABLE_SHARED_SPACE_SYNC,
        table_backends = crate::table_names::TABLE_S3_BACKENDS,
        col_backend_id = crate::table_names::COL_S3_BACKENDS_ID,
        col_backend_origin = crate::table_names::COL_S3_BACKENDS_ORIGIN_TYPE,
        col_table = crate::table_names::COL_SHARED_SPACE_SYNC_TABLE_NAME,
        col_pks = crate::table_names::COL_SHARED_SPACE_SYNC_ROW_PKS,
    );
    let rows = crate::database::core::select_with_crdt(
        sql,
        vec![
            serde_json::Value::String(crate::table_names::TABLE_S3_BACKENDS.to_string()),
            serde_json::Value::String(backend_id.to_string()),
        ],
        db,
    )
    .map_err(|e| FileSyncCommandError::ProviderError(format!("space-binding lookup: {e}")))?;

    if rows.len() > 1 {
        return Err(FileSyncCommandError::InvalidConfig(format!(
            "backend {backend_id} is bound to multiple spaces via haex_shared_space_sync — refusing to pick one"
        )));
    }
    Ok(rows
        .first()
        .map(|row| crate::database::row::get_string(row, 0)))
}

/// Enforce that a cloud sync-rule's `spaceId` matches the space (if any)
/// the target backend is actually shared for.
///
/// A backend shared into a space (`origin_type = 'shared_from_space'`) is
/// bound to exactly the space that shared it. A sync rule pointing at that
/// backend but carrying a different (or absent) `spaceId` would seal its
/// files under the wrong epoch key. An owner-only backend (no share) has no
/// space binding at all — personal buckets aren't space-scoped, so a
/// `spaceId` set against one is rejected too rather than silently accepted.
fn verify_cloud_space_binding(
    backend_id: &str,
    config_space_id: Option<&str>,
    db: &crate::database::DbConnection,
) -> Result<(), FileSyncCommandError> {
    let bound_space_id = shared_backend_space_id(backend_id, db)?;
    match (bound_space_id.as_deref(), config_space_id) {
        (None, None) => Ok(()),
        (Some(bound), Some(cfg)) if bound == cfg => Ok(()),
        (None, Some(cfg)) => Err(FileSyncCommandError::InvalidConfig(format!(
            "backend {backend_id} is not shared for any space, but sync-rule spaceId is '{cfg}'"
        ))),
        (Some(bound), cfg) => Err(FileSyncCommandError::InvalidConfig(format!(
            "backend {backend_id} is shared for space '{bound}', but sync-rule spaceId is {cfg:?}"
        ))),
    }
}

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
    rule_id: &str,
) -> Result<Arc<dyn SyncProvider>, FileSyncCommandError> {
    match provider_type {
        "local" => {
            let path = config.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                FileSyncCommandError::InvalidConfig("local provider requires 'path'".into())
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
                    FileSyncCommandError::InvalidConfig("peer provider requires 'ucanToken'".into())
                })?
                .to_string();

            let endpoint = state.peer_storage.clone();
            let provider =
                PeerProvider::new(endpoint, endpoint_id, relay_url, base_path, ucan_token);
            Ok(Arc::new(provider))
        }
        "cloud" => {
            assemble_cloud_provider(
                config,
                rule_id,
                is_target,
                DbConnection(state.db.0.clone()),
                state.vault_key.clone(),
            )
            .await
        }
        _ => Err(FileSyncCommandError::InvalidConfig(format!(
            "Unknown provider type: {provider_type}"
        ))),
    }
}

/// Build the full cloud-provider stack for one sync rule.
///
/// Extracted from `create_provider` so the space-scoped wiring is
/// unit-testable without a full `AppState`. Consumes only the two `AppState`
/// fields the cloud arm actually needs — the DB connection and the
/// own-vault `vault_key` slot — so tests can plumb both by hand.
///
/// Layering (Phase 4 Round F3b, locked):
///
/// `ScopedProvider(SpaceContentSyncProvider(CloudProvider))`
///
/// Rationale for outermost `ScopedProvider`: F3a's `SpaceContentSyncProvider`
/// translates user-facing paths through the encryption pipeline; if the LIST
/// guard sat *inside* it, cross-scope keys the decorator surfaces up would
/// slip past the guard. Wrapping the guard outside the encryption decorator
/// strips them at the observable boundary.
///
/// Owner-only rules (no `spaceId`) skip the `ScopedProvider` wrap and hit
/// [`wrap_cloud_with_encryption_if_configured`]'s own-vault branch, matching
/// the F2 shape unchanged.
/// Extract a normalized `spaceId` from a sync-rule config.
///
/// - **Absent, `null`, empty string** → `Ok(None)` (owner-only rule).
/// - **Non-empty string** → `Ok(Some(&str))`.
/// - **Any other JSON shape** → `Err(InvalidConfig)`. Distinguishing "absent"
///   from "present but wrong type" prevents a caller from passing
///   `{ "spaceId": 42 }` against an owner-only backend as if no spaceId were
///   set — a mistake that would silently mint owner credentials against a
///   rule the caller believed was space-scoped.
///
/// Shared by [`assemble_cloud_provider`],
/// [`wrap_cloud_with_encryption_if_configured`], and
/// [`wrap_cloud_with_scoped_provider_if_configured`] so the "empty string ==
/// None" normalization is enforced in exactly one place.
fn space_id_from_config(config: &serde_json::Value) -> Result<Option<&str>, FileSyncCommandError> {
    match config.get("spaceId") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) if s.is_empty() => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.as_str())),
        Some(other) => Err(FileSyncCommandError::InvalidConfig(format!(
            "spaceId must be a string, got {other}"
        ))),
    }
}

pub(crate) async fn assemble_cloud_provider(
    config: &serde_json::Value,
    rule_id: &str,
    is_target: bool,
    db: DbConnection,
    vault_key_slot: std::sync::Arc<std::sync::Mutex<Option<zeroize::Zeroizing<[u8; 32]>>>>,
) -> Result<Arc<dyn SyncProvider>, FileSyncCommandError> {
    let backend_id = config
        .get("backendId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            FileSyncCommandError::InvalidConfig("cloud provider requires 'backendId'".into())
        })?;
    let space_id = space_id_from_config(config)?;
    verify_cloud_space_binding(backend_id, space_id, &db)?;

    let prefix = config
        .get("prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let bucket_override = config
        .get("bucket")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

    // Shared-backend path (spaceId present): unseal the current viewer's
    // per-member `ScopedCred` for this (space, backend) and thread it into
    // the backend factory so the backend never sees the owner's admin
    // creds. Owner-only path skips the lookup entirely — the config row
    // still carries the owner's credentials in that case.
    let scoped_cred_override = match space_id {
        None => None,
        Some(sid) => Some(load_scoped_cred_for_shared_backend(&db, sid, backend_id)?),
    };

    let backend = crate::remote_storage::commands::get_backend_instance_from_db_with_overrides(
        &db,
        backend_id,
        bucket_override,
        scoped_cred_override.as_ref(),
    )
    .await
    .map_err(|e| FileSyncCommandError::ProviderError(e.to_string()))?;

    // Auto-create the bucket only when this provider is the sync target —
    // a missing *source* bucket is almost always a typo or stale config
    // and should surface as an error instead.
    if is_target {
        backend
            .ensure_container()
            .await
            .map_err(|e| FileSyncCommandError::ProviderError(e.to_string()))?;
    }

    let cloud = CloudProvider::new(backend, prefix.clone());
    let inner: Arc<dyn SyncProvider> = Arc::new(cloud);
    let encrypting =
        wrap_cloud_with_encryption_if_configured(inner, config, rule_id, db, vault_key_slot)?;
    wrap_cloud_with_scoped_provider_if_configured(encrypting, config, prefix)
}

/// Load and unseal the current viewer's `ScopedCred` for a
/// `(space_id, backend_id)` pair.
///
/// The viewer DID is resolved via
/// [`crate::owner_sync::scope::resolve_local_member_did_for_space`] — the
/// LOCAL identity that joined `space_id`. Per CLAUDE.md, a user can join a
/// shared space with an identity distinct from the vault owner's, so using
/// `resolve_vault_owner_did` here would silently miss non-vault-owner
/// memberships (Task 4's fanout writes the `haex_s3_shared_access` row
/// keyed by the identity that actually joined, not the vault owner).
///
/// Failure modes:
/// - No local identity joined the space → `InvalidConfig` (setup bug: this
///   vault is not a member of the space it claims to sync).
/// - No `haex_s3_shared_access` row for the resolved DID → `NotShared` —
///   semantically distinct from a config error: this viewer's capability
///   has been revoked (or was never granted), and the UI should surface a
///   share-revoked / re-enrollment prompt.
///
/// Kept `pub(crate)` so the wire-up seam is testable from
/// [`crate::file_sync::commands_tests`] without a full `AppState`.
pub(crate) fn load_scoped_cred_for_shared_backend(
    db: &DbConnection,
    space_id: &str,
    backend_id: &str,
) -> Result<crate::remote_storage::iam_adapter::ScopedCred, FileSyncCommandError> {
    let member_did = crate::database::core::with_connection(db, |conn| {
        crate::owner_sync::scope::resolve_local_member_did_for_space(conn, space_id).map_err(|e| {
            crate::database::error::DatabaseError::QueryError {
                reason: e.to_string(),
            }
        })
    })
    .map_err(|e| FileSyncCommandError::ProviderError(format!("resolve viewer DID: {e}")))?
    .ok_or_else(|| {
        FileSyncCommandError::InvalidConfig(format!(
            "cannot load shared-backend credentials: this vault has no local identity \
             joined to space={space_id}"
        ))
    })?;

    let row = crate::remote_storage::shared_access::find_shared_access(
        db,
        space_id,
        backend_id,
        &member_did,
    )
    .map_err(|e| FileSyncCommandError::ProviderError(format!("shared-access lookup: {e}")))?
    .ok_or_else(|| FileSyncCommandError::NotShared {
        space_id: space_id.to_string(),
        backend_id: backend_id.to_string(),
        member_did: member_did.clone(),
    })?;

    let epoch_key = crate::file_sync::crypto::key_resolver::resolve_key(space_id, row.epoch, db)
        .map_err(|e| {
            FileSyncCommandError::ProviderError(format!(
                "resolve epoch key (space={space_id} epoch={}): {e}",
                row.epoch
            ))
        })?;

    crate::remote_storage::shared_access::crypto::open_scoped_cred(&row.encrypted_cred, &epoch_key)
        .map_err(|e| FileSyncCommandError::ProviderError(format!("unseal ScopedCred: {e}")))
}

/// Install the outermost `ScopedProvider` LIST guard around an
/// already-encryption-wrapped cloud provider when `config.spaceId` names a
/// space. Owner-only rules (absent / null / empty spaceId) pass through
/// unchanged — the F3a scope-cache treatment stays sufficient there.
///
/// The wrap decision mirrors [`wrap_cloud_with_encryption_if_configured`] to
/// keep the two seams observably in lockstep — same config, same fallthrough
/// branches. See `assemble_cloud_provider` for the layering rationale.
pub(crate) fn wrap_cloud_with_scoped_provider_if_configured(
    inner: Arc<dyn SyncProvider>,
    config: &serde_json::Value,
    prefix: String,
) -> Result<Arc<dyn SyncProvider>, FileSyncCommandError> {
    match space_id_from_config(config)? {
        Some(_) => Ok(Arc::new(
            crate::file_sync::scoped_provider::ScopedProvider::new(inner, prefix),
        )),
        None => Ok(inner),
    }
}

/// Wrap a built cloud provider in an encrypting decorator.
///
/// The `config.spaceId` field routes between the two grant scopes:
///
/// - **Absent / null / empty string** → own-vault path via
///   [`EncryptingSyncProvider`]. KEK is the `vault_key` slot value,
///   wrapping a per-object DEK carried inside each `own/<hex>.m`
///   sidecar. An empty slot at first seal/open surfaces
///   `ProviderCryptoError::OwnVaultNotWired` — clear error, not silent
///   corruption.
/// - **Non-empty string** → space-scoped path via
///   [`SpaceContentSyncProvider`]. KEK is the current MLS epoch key for
///   the space, wrapping the same per-object DEK inside a
///   `space-<space_id>/<hex>.m` sidecar. Content object bytes live at
///   the same `content/o/<hex>` path either way, so a file shared into
///   N spaces is still one physical object.
///
/// A `spaceId` field of the wrong JSON shape (a non-string) is rejected
/// rather than silently falling through to own-vault — that fallthrough
/// would ship an unshared object under a rule the caller believed was
/// space-scoped.
///
/// Extracted from `create_provider` so the wrapping decision can be
/// unit-tested without a full `AppState`. Kept `pub(crate)` so the tests
/// in [`crate::file_sync::crypto::tests`] can call it directly.
pub(crate) fn wrap_cloud_with_encryption_if_configured(
    inner: Arc<dyn SyncProvider>,
    config: &serde_json::Value,
    rule_id: &str,
    db: DbConnection,
    vault_key_slot: std::sync::Arc<std::sync::Mutex<Option<zeroize::Zeroizing<[u8; 32]>>>>,
) -> Result<Arc<dyn SyncProvider>, FileSyncCommandError> {
    match space_id_from_config(config)? {
        None => Ok(Arc::new(EncryptingSyncProvider::new(
            inner,
            rule_id,
            db,
            vault_key_slot,
        ))),
        Some(space_id) => Ok(Arc::new(SpaceContentSyncProvider::new(
            inner,
            rule_id,
            db,
            space_id.to_string(),
            Arc::new(crate::file_sync::crypto::MlsSpaceKeyResolver),
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

    // Stop any existing loop for this rule. Drain the handle under the lock,
    // then await it OUTSIDE the lock — `auto_disable_rule` re-enters the
    // same mutex via `deregister`, so awaiting under the lock would deadlock
    // if the task is currently waiting to deregister itself.
    let prev_handle = {
        let mut manager = state.sync_manager.lock().await;
        manager.take_stop(&rule_id)
    };
    if let Some(handle) = prev_handle {
        await_sync_handle(&rule_id, handle).await;
    }

    let source = create_provider(&source_type, &source_config, &state, false, &rule_id)
        .await
        .inspect_err(|e| eprintln!("[FileSync] Failed to create source provider: {e}"))?;
    let target = create_provider(&target_type, &target_config, &state, true, &rule_id)
        .await
        .inspect_err(|e| eprintln!("[FileSync] Failed to create target provider: {e}"))?;

    let cancel = CancellationToken::new();
    let (trigger_sender, trigger_receiver) = tokio::sync::mpsc::channel::<()>(16);
    let db = crate::database::DbConnection(state.db.0.clone());
    let rule_id_clone = rule_id.clone();

    // Start file watcher for local providers — directly triggers sync loop
    if target_type == "local" {
        if let Some(path) = target_config.get("path").and_then(|v| v.as_str()) {
            let _ = state.file_watcher.watch(
                app.clone(),
                rule_id.clone(),
                path.to_string(),
                Some(trigger_sender.clone()),
            );
        }
    }
    if source_type == "local" {
        if let Some(path) = source_config.get("path").and_then(|v| v.as_str()) {
            let watcher_key = format!("{}_source", rule_id);
            let _ = state.file_watcher.watch(
                app.clone(),
                watcher_key,
                path.to_string(),
                Some(trigger_sender.clone()),
            );
        }
    }

    let app_clone = app.clone();
    let cancel_for_task = cancel.clone();
    // Use tokio::spawn so the returned JoinHandle is `tokio::task::JoinHandle`,
    // matching the type retained by `SyncManager` for await-on-stop. The
    // futures spawned here are tokio-native (mpsc/select!) so this is safe.
    let handle = tokio::spawn(async move {
        run_sync_loop(
            source,
            target,
            dir,
            del,
            rule_id_clone,
            Duration::from_secs(interval_seconds),
            cancel_for_task,
            trigger_receiver,
            db,
            app_clone,
        )
        .await;
    });

    // Register after spawning so the JoinHandle is captured. Status queries
    // observe the rule as running once the lock is acquired below.
    {
        let mut manager = state.sync_manager.lock().await;
        manager.register(rule_id.clone(), cancel, trigger_sender.clone(), handle);
    }

    Ok(())
}

/// Stop syncing for a specific rule.
#[tauri::command(rename_all = "camelCase")]
pub async fn file_sync_stop_rule(
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<(), FileSyncCommandError> {
    // Drain handle under the lock, await outside — see `take_stop` doc.
    let handle = {
        let mut manager = state.sync_manager.lock().await;
        if !manager.is_running(&rule_id) {
            return Err(FileSyncCommandError::NotRunning(rule_id));
        }
        manager.take_stop(&rule_id)
    };
    if let Some(handle) = handle {
        await_sync_handle(&rule_id, handle).await;
    }

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

    let source = create_provider(&source_type, &source_config, &state, false, &rule_id).await?;
    let target = create_provider(&target_type, &target_config, &state, true, &rule_id).await?;

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
pub async fn file_sync_stop_all(state: State<'_, AppState>) -> Result<(), FileSyncCommandError> {
    // Drain all handles under the lock, await outside — see `take_stop` doc.
    let drained = {
        let mut manager = state.sync_manager.lock().await;
        manager.take_stop_all()
    };
    for (rule_id, handle) in drained {
        await_sync_handle(&rule_id, handle).await;
    }

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
        &state.column_sig_key_cache,
    )
    .map_err(|e| FileSyncCommandError::Internal(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod commands_tests;

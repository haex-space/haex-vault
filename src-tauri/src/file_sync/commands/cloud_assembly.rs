//! Cloud-provider assembly for file-sync rules.

use std::sync::Arc;

use crate::database::DbConnection;
use crate::file_sync::cloud_provider::CloudProvider;
use crate::file_sync::provider::SyncProvider;

use super::{wrap_cloud_with_encryption_if_configured, FileSyncCommandError};

/// Extract a normalized `spaceId` from a sync-rule config.
///
/// - Absent, `null`, empty string → `Ok(None)` (owner-only rule).
/// - Non-empty string → `Ok(Some(&str))`.
/// - Any other JSON shape → `Err(InvalidConfig)`.
pub(super) fn space_id_from_config(
    config: &serde_json::Value,
) -> Result<Option<&str>, FileSyncCommandError> {
    match config.get("spaceId") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) if s.is_empty() => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.as_str())),
        Some(other) => Err(FileSyncCommandError::InvalidConfig(format!(
            "spaceId must be a string, got {other}"
        ))),
    }
}

/// Build the full cloud-provider stack for one sync rule.
///
/// Space rules produce
/// `ScopedProvider(SpaceContentSyncProvider(CloudProvider))`; owner-only
/// rules omit the scoped guard and retain the own-vault encryption path.
pub(crate) async fn assemble_cloud_provider(
    config: &serde_json::Value,
    rule_id: &str,
    is_target: bool,
    db: DbConnection,
    vault_key_slot: Arc<std::sync::Mutex<Option<zeroize::Zeroizing<[u8; 32]>>>>,
) -> Result<Arc<dyn SyncProvider>, FileSyncCommandError> {
    let backend_id = config
        .get("backendId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            FileSyncCommandError::InvalidConfig("cloud provider requires 'backendId'".into())
        })?;
    let space_id = space_id_from_config(config)?;
    super::verify_cloud_space_binding(backend_id, space_id, &db)?;

    let prefix = config
        .get("prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let bucket_override = config
        .get("bucket")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

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

/// Load and unseal the local member's credential for one shared backend.
pub(crate) fn load_scoped_cred_for_shared_backend(
    db: &DbConnection,
    space_id: &str,
    backend_id: &str,
) -> Result<crate::remote_storage::iam_adapter::ScopedCred, FileSyncCommandError> {
    let member_did = crate::owner_sync::scope::resolve_local_member_did_for_space(db, space_id)
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

/// Install the outermost scoped path guard for space-scoped rules.
pub(crate) fn wrap_cloud_with_scoped_provider_if_configured(
    inner: Arc<dyn SyncProvider>,
    config: &serde_json::Value,
    prefix: String,
) -> Result<Arc<dyn SyncProvider>, FileSyncCommandError> {
    match space_id_from_config(config)? {
        Some(_) => Ok(Arc::new(crate::file_sync::ScopedProvider::new(
            inner, prefix,
        ))),
        None => Ok(inner),
    }
}

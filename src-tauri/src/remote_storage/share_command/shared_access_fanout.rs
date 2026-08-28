//! Sealed shared-access fanout and rollback for the share flow.

use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::database::row::get_string;
use crate::remote_storage::error::StorageError;
use crate::remote_storage::iam_adapter::IamAdapter;
use crate::table_names::{TABLE_S3_BACKENDS, TABLE_SHARED_SPACE_SYNC};

use super::EpochResolver;

/// Seal the scoped credential once per current space member.
pub(super) fn write_shared_access_fanout(
    db: &crate::database::DbConnection,
    hlc_service: &crate::crdt::hlc::HlcService,
    key_cache: &crate::crdt::column_sig::key_cache::SpaceKeyCache,
    epoch_resolver: &dyn EpochResolver,
    space_id: &str,
    child_backend_id: &str,
    scoped_cred: &crate::remote_storage::iam_adapter::ScopedCred,
) -> Result<(), StorageError> {
    let (epoch, epoch_key) =
        epoch_resolver
            .resolve_latest(db, space_id)
            .map_err(|e| StorageError::Internal {
                reason: format!("resolve latest MLS epoch key for space {space_id}: {e}"),
            })?;

    let member_dids = load_space_member_dids(db, space_id)?;
    if member_dids.is_empty() {
        return Ok(());
    }

    let hlc_local = std::sync::Mutex::new(hlc_service.clone());
    let hlc_guard = hlc_local.lock().map_err(|e| StorageError::Internal {
        reason: format!("hlc local mutex poisoned: {e}"),
    })?;

    for did in &member_dids {
        crate::remote_storage::shared_access::upsert_sealed_scoped_cred(
            db,
            &hlc_guard,
            key_cache,
            &uuid::Uuid::new_v4().to_string(),
            space_id,
            child_backend_id,
            did,
            scoped_cred,
            epoch,
            &epoch_key,
            None,
        )
        .map_err(|e| StorageError::Internal {
            reason: format!("upsert sealed shared_access row for member {did}: {e}"),
        })?;
    }
    Ok(())
}

/// Load every member DID for a space through the CRDT query chokepoint.
fn load_space_member_dids(
    db: &crate::database::DbConnection,
    space_id: &str,
) -> Result<Vec<String>, StorageError> {
    let rows = select_with_crdt(
        "SELECT DISTINCT i.did \
         FROM haex_identities i \
         JOIN haex_space_members m ON m.identity_id = i.id \
         WHERE m.space_id = ?1"
            .to_string(),
        vec![serde_json::Value::String(space_id.to_string())],
        db,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: format!("load member dids for space {space_id}: {e}"),
    })?;
    Ok(rows
        .iter()
        .map(|row| get_string(row, 0))
        .filter(|did| !did.is_empty())
        .collect())
}

/// Best-effort rollback after a shared-access fanout failure.
pub(super) async fn rollback_child_backend_and_iam(
    db: &crate::database::DbConnection,
    hlc_service: &crate::crdt::hlc::HlcService,
    key_cache: &crate::crdt::column_sig::key_cache::SpaceKeyCache,
    adapter: &dyn IamAdapter,
    child_backend_id: &str,
    space_id: &str,
    iam_user_name: &str,
) {
    {
        let hlc_local = std::sync::Mutex::new(hlc_service.clone());
        let hlc_guard = match hlc_local.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(
                    child_backend_id = %child_backend_id,
                    error = %e,
                    "hlc mutex poisoned during shared-access rollback; leaving DB rows in place"
                );
                return;
            }
        };

        let delete_shared = format!(
            "DELETE FROM {} WHERE space_id = ?1 AND backend_id = ?2",
            crate::table_names::TABLE_S3_SHARED_ACCESS,
        );
        if let Err(e) = execute_with_crdt(
            delete_shared,
            vec![
                serde_json::Value::String(space_id.to_string()),
                serde_json::Value::String(child_backend_id.to_string()),
            ],
            db,
            &hlc_guard,
            key_cache,
        ) {
            tracing::error!(
                child_backend_id = %child_backend_id,
                error = %e,
                "shared-access rollback DELETE failed"
            );
        }

        let delete_mapping = format!(
            "DELETE FROM {TABLE_SHARED_SPACE_SYNC} \
             WHERE table_name = ?1 AND space_id = ?2 AND json_extract(row_pks, '$[0]') = ?3"
        );
        if let Err(e) = execute_with_crdt(
            delete_mapping,
            vec![
                serde_json::Value::String(TABLE_S3_BACKENDS.to_string()),
                serde_json::Value::String(space_id.to_string()),
                serde_json::Value::String(child_backend_id.to_string()),
            ],
            db,
            &hlc_guard,
            key_cache,
        ) {
            tracing::error!(child_backend_id = %child_backend_id, error = %e, "mapping rollback DELETE failed");
        }

        if let Err(e) = execute_with_crdt(
            format!("DELETE FROM {TABLE_S3_BACKENDS} WHERE id = ?1"),
            vec![serde_json::Value::String(child_backend_id.to_string())],
            db,
            &hlc_guard,
            key_cache,
        ) {
            tracing::error!(child_backend_id = %child_backend_id, error = %e, "child backend rollback DELETE failed");
        }
    }

    if let Err(e) = adapter.delete_scoped_user(iam_user_name).await {
        tracing::warn!(
            user_name = %iam_user_name,
            error = %e,
            "IAM rollback failed after shared-access fanout failure; scoped user may be orphaned"
        );
    }
}

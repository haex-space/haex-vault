//! `revoke_storage_share` Tauri command — Task F1 of the S3-bucket sharing
//! feature. Symmetric counterpart to
//! [`crate::remote_storage::share_command::share_storage_backend`]: given a
//! shared-backend row id, tear down the scoped IAM user at the provider,
//! then hard-delete both the `haex_s3_backends` row and its
//! `haex_shared_space_sync` mapping.
//!
//! The DB deletes go through the CRDT helpers, so the resulting
//! `haex_deleted_rows` entries propagate to space members even if they are
//! currently offline (see memory `delete-log-not-tombstones`). No tombstone
//! column is used.
//!
//! # Order of operations
//!
//! 1. Load the shared row + verify `origin_type = 'shared_from_space'`.
//! 2. Extract `iamUserName` from the row's config JSON — the access-key id
//!    is discovered from the provider at revoke time (Round F3b sanitised
//!    cred material out of the config).
//! 3. Load the parent backend row (data-integrity check).
//! 4. Load the [`crate::remote_storage::iam_admin_creds::IamAdminCred`] for the parent id.
//! 5. Delete the scoped IAM user at the provider — the adapter enumerates
//!    and deletes every attached access-key before removing the inline
//!    policy + user. Idempotent: `NotFound` is treated as success.
//! 6. Delete shared-access rows, the mapping row, then the s3_backends row.
//!
//! **IAM-first ordering is deliberate**: if the IAM delete fails with
//! anything other than `NotFound`, the DB rows are left intact so the user
//! can retry. Deleting DB rows first would leave the IAM user
//! provider-side-orphaned with no vault record of how to revoke it.

use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

use crate::critical::CriticalFailureCode;
use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::database::row::get_string;
use crate::remote_storage::error::StorageError;
use crate::remote_storage::iam_adapter::{IamAdapter, IamAdapterError};
use crate::remote_storage::iam_admin_creds;
use crate::remote_storage::share_command::{
    provider_flavor_from, DefaultIamAdapterFactory, IamAdapterFactory,
};
use crate::table_names::{
    COL_S3_BACKENDS_CONFIG, COL_S3_BACKENDS_ID, TABLE_S3_BACKENDS, TABLE_S3_SHARED_ACCESS,
    TABLE_SHARED_SPACE_SYNC,
};
use crate::AppState;

// ---------------------------------------------------------------------------
// Request payload
// ---------------------------------------------------------------------------

/// Arguments to `revoke_storage_share`. Symmetrical to E1's share flow — the
/// only input needed to revoke is the shared row's id; the parent backend id,
/// scoped IAM user, and admin cred are all derivable from the DB.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeStorageShareArgs {
    /// The `haex_s3_backends.id` of the `origin_type = 'shared_from_space'`
    /// row to revoke.
    pub shared_backend_id: String,
}

// ---------------------------------------------------------------------------
// Internal view of the shared row
// ---------------------------------------------------------------------------

/// Minimal projection of the shared row required by the revoke flow.
///
/// Since Round F3b, the child backend row's config JSON carries only
/// structural fields plus `iamUserName` — the scoped access-key id is
/// discovered from the IAM provider at revoke time via
/// [`IamAdapter::list_access_key_ids`].
#[derive(Debug)]
struct SharedRow {
    parent_backend_id: String,
    iam_user_name: String,
}

fn load_shared_row(
    db: &crate::database::DbConnection,
    shared_backend_id: &str,
) -> Result<SharedRow, StorageError> {
    // Include origin_type + parent_backend_id in the select so we can
    // distinguish "not found" from "owned row (wrong path)" from
    // "shared row" without three separate queries.
    let sql = format!(
        "SELECT origin_type, parent_backend_id, {COL_S3_BACKENDS_CONFIG} \
         FROM {TABLE_S3_BACKENDS} \
         WHERE {COL_S3_BACKENDS_ID} = ?1"
    );
    let rows = select_with_crdt(
        sql,
        vec![serde_json::Value::String(shared_backend_id.to_string())],
        db,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: format!("load shared backend row: {e}"),
    })?;

    let row = rows.first().ok_or_else(|| StorageError::StorageNotFound {
        storage_id: shared_backend_id.to_string(),
    })?;

    let origin_type = get_string(row, 0);
    if origin_type != "shared_from_space" {
        return Err(StorageError::NotAShareRow { origin_type });
    }

    let parent_backend_id = get_string(row, 1);
    if parent_backend_id.is_empty() {
        // shared_from_space rows must carry a parent id — if empty the row
        // is corrupt. Surface as ParentBackendMissing so operators see the
        // same actionable variant.
        return Err(StorageError::ParentBackendMissing {
            parent_backend_id: String::new(),
        });
    }

    let config_str = get_string(row, 2);
    let config: serde_json::Value =
        serde_json::from_str(&config_str).map_err(|e| StorageError::InvalidConfig {
            reason: format!("shared backend config JSON parse failed: {e}"),
        })?;

    let iam_user_name = config
        .get("iamUserName")
        .and_then(|v| v.as_str())
        .ok_or_else(|| StorageError::InvalidConfig {
            reason: "shared backend config missing 'iamUserName' field".to_string(),
        })?
        .to_string();

    Ok(SharedRow {
        parent_backend_id,
        iam_user_name,
    })
}

/// Assert the parent backend row referenced by the shared row still exists.
/// We don't need any of its columns for the IAM revoke (the admin cred carries
/// its own `provider_type`), but the check catches data-corruption/race cases
/// before we go touch the provider.
///
/// This is a pure existence check — we do NOT filter on `origin_type = 'owned'`.
/// Origin correctness of the parent row is trusted to be enforced elsewhere
/// (share_command's `load_owner_backend` and the shape of the `parent_backend_id`
/// FK). Filtering on origin here would surface a misleading `ParentBackendMissing`
/// error when the row IS present but corrupted, obscuring the actual data issue.
fn assert_parent_exists(
    db: &crate::database::DbConnection,
    parent_backend_id: &str,
) -> Result<(), StorageError> {
    let sql = format!("SELECT 1 FROM {TABLE_S3_BACKENDS} WHERE {COL_S3_BACKENDS_ID} = ?1");
    let rows = select_with_crdt(
        sql,
        vec![serde_json::Value::String(parent_backend_id.to_string())],
        db,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: format!("load parent backend row: {e}"),
    })?;

    if rows.is_empty() {
        return Err(StorageError::ParentBackendMissing {
            parent_backend_id: parent_backend_id.to_string(),
        });
    }
    Ok(())
}

/// Hard-delete the shared_access fanout, the mapping row, and the
/// s3_backends row. Order mirrors `share_command::rollback_child_backend_and_iam`:
/// shared_access → mapping → child backend.
///
/// **shared_access first**: Round F3b's share flow writes one
/// `haex_s3_shared_access` row per space member. If revoke does not delete
/// them, the rows continue to sync via `SPACE_SCOPED_CRDT_TABLES` to every
/// current and future member — a persistent leak of sealed ScopedCred rows
/// referencing a child backend that no longer exists.
///
/// **Mapping before backend** — see
/// `docs/plans/2026-07-04-s3-bucket-sharing-via-spaces-design.md` §6:
///
/// - If the mapping DELETE succeeds and the s3_backends DELETE then fails,
///   the mapping is orphaned pointing at a still-live row. Subsequent revoke
///   attempts will find the row via `load_shared_row` and re-drive the IAM +
///   DB cleanup — safe retry surface.
/// - If we did s3_backends first, a mapping DELETE failure would leave the
///   mapping pointing at a nonexistent row → the shared-space sync engine
///   would try to sync a phantom, and E1's `find_existing_share` idempotency
///   check would incorrectly report "already shared".
///
/// Each `execute_with_crdt` opens its own SQLite tx, so we can't wrap the
/// three DELETEs in a single atomic transaction. That's the same limitation
/// E1's `persist_shared_backend` operates under.
fn delete_share_rows(
    db: &crate::database::DbConnection,
    hlc_service: &crate::crdt::hlc::HlcService,
    key_cache: &crate::crdt::column_sig::key_cache::SpaceKeyCache,
    shared_backend_id: &str,
) -> Result<(), StorageError> {
    let hlc_local = std::sync::Mutex::new(hlc_service.clone());
    let hlc_guard = hlc_local.lock().map_err(|e| StorageError::Internal {
        reason: format!("hlc local mutex poisoned: {e}"),
    })?;

    // 1. shared_access rows first — see doc-comment above. `backend_id`
    //    names the child backend's immutable UUID primary key, so it is the
    //    stable cleanup key even if a prior partial revoke already removed
    //    the mapping row. Requiring the mapping's space_id here would make a
    //    retry leak sealed credentials forever.
    let delete_shared_access =
        format!("DELETE FROM {TABLE_S3_SHARED_ACCESS} WHERE backend_id = ?1");
    execute_with_crdt(
        delete_shared_access,
        vec![serde_json::Value::String(shared_backend_id.to_string())],
        db,
        &hlc_guard,
        key_cache,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: format!("delete haex_s3_shared_access: {e}"),
    })?;

    // 2. Delete the mapping. `row_pks` is a JSON array with the
    //    shared-backend id at index 0.
    let delete_mapping = format!(
        "DELETE FROM {TABLE_SHARED_SPACE_SYNC} \
         WHERE table_name = ?1 \
           AND json_extract(row_pks, '$[0]') = ?2"
    );
    execute_with_crdt(
        delete_mapping,
        vec![
            serde_json::Value::String(TABLE_S3_BACKENDS.to_string()),
            serde_json::Value::String(shared_backend_id.to_string()),
        ],
        db,
        &hlc_guard,
        key_cache,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: format!("delete haex_shared_space_sync: {e}"),
    })?;

    let delete_backend = format!("DELETE FROM {TABLE_S3_BACKENDS} WHERE {COL_S3_BACKENDS_ID} = ?1");
    if let Err(e) = execute_with_crdt(
        delete_backend,
        vec![serde_json::Value::String(shared_backend_id.to_string())],
        db,
        &hlc_guard,
        key_cache,
    ) {
        // Mapping is already gone; the s3_backends row is still present.
        // On the user's next revoke attempt, `load_shared_row` will still
        // find the row and re-drive IAM+DB cleanup — the orphan is
        // transient. Log so an operator can see if it accumulates.
        tracing::error!(
            shared_backend_id = %shared_backend_id,
            error = %e,
            "s3_backends DELETE failed after mapping DELETE succeeded; \
             shared row is orphaned. Retry revoke to clean up."
        );
        return Err(StorageError::DatabaseError {
            reason: format!("delete haex_s3_backends: {e}"),
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri command entry point
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn revoke_storage_share(
    state: State<'_, AppState>,
    shared_backend_id: String,
) -> Result<(), StorageError> {
    // Snapshot the hlc guard once, hand a plain `&HlcService` into the
    // testable core — mirrors E1's structure so tests can bypass
    // AppState.lock_or_fail.
    let hlc_snapshot = {
        let guard = state.lock_or_fail(
            &state.hlc,
            CriticalFailureCode::HlcMutexPoisoned,
            "remote_storage::revoke_command::revoke_storage_share",
            serde_json::json!({}),
        )?;
        guard.clone()
    };
    revoke_storage_share_core(
        &state.db,
        &hlc_snapshot,
        &state.column_sig_key_cache,
        RevokeStorageShareArgs { shared_backend_id },
        &DefaultIamAdapterFactory,
    )
    .await
}

/// Testable core form: takes raw `db` + `hlc` and a pluggable adapter
/// factory so unit tests can inject a mock without hitting AWS/Wasabi.
///
/// # Flow
///
/// See module doc-comment for the eight-step ordering.
pub(crate) async fn revoke_storage_share_core(
    db: &crate::database::DbConnection,
    hlc_service: &crate::crdt::hlc::HlcService,
    key_cache: &crate::crdt::column_sig::key_cache::SpaceKeyCache,
    args: RevokeStorageShareArgs,
    factory: &dyn IamAdapterFactory,
) -> Result<(), StorageError> {
    // 1. Load the shared row + assert origin_type.
    let shared = load_shared_row(db, &args.shared_backend_id)?;

    // 2. Assert parent exists (data-integrity check before we touch IAM).
    assert_parent_exists(db, &shared.parent_backend_id)?;

    // 3. Load the IAM admin cred for the parent backend.
    let cred = match iam_admin_creds::load(db, &shared.parent_backend_id) {
        Ok(Some(cred)) => cred,
        Ok(None) => {
            return Err(StorageError::IamAdminCredMissing {
                storage_id: shared.parent_backend_id.clone(),
            });
        }
        Err(e) => {
            return Err(StorageError::DatabaseError {
                reason: format!("load iam admin cred: {e}"),
            });
        }
    };

    // 4. Build the IAM adapter for the cred's provider.
    let flavor = provider_flavor_from(&cred)?;
    let adapter: Arc<dyn IamAdapter> =
        factory
            .build(&cred, flavor)
            .map_err(|e| StorageError::IamOperationFailed {
                operation: "build_adapter".to_string(),
                reason: e.to_string(),
            })?;

    // 5. Provider-side delete. Round F3b stripped the access-key id out of
    //    the child config's JSON, so the adapter is the only place that
    //    knows which keys the scoped user still has. `delete_scoped_user`
    //    now takes only the user_name — the adapter enumerates via
    //    `list_access_key_ids` internally and deletes each attached key
    //    before removing the inline policy + user.
    //
    //    `NotFound` is idempotent — the user may already be gone (manual
    //    cleanup, prior partial revoke, or the adapter's own best-effort
    //    inner delete swallowed the outer NoSuchEntity). Everything else
    //    halts the flow before we touch the DB, so a retry can find the
    //    same rows.
    match adapter.delete_scoped_user(&shared.iam_user_name).await {
        Ok(()) => {}
        Err(IamAdapterError::NotFound) => {
            tracing::info!(
                iam_user_name = %shared.iam_user_name,
                "IAM user already absent at provider; proceeding with DB cleanup"
            );
        }
        Err(e) => {
            return Err(StorageError::IamOperationFailed {
                operation: "delete_scoped_user".to_string(),
                reason: e.to_string(),
            });
        }
    }

    // 6. DB cleanup: shared_access → mapping → s3_backends row.
    delete_share_rows(db, hlc_service, key_cache, &args.shared_backend_id)?;

    Ok(())
}

#[cfg(test)]
mod tests;

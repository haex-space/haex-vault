//! `share_storage_backend` Tauri command — Task E1 of the S3-bucket sharing
//! feature. Wires Phases A–D together: reads the owner's backend row, loads
//! or refreshes the IAM-admin credential (via `iam_admin_creds`), asks the
//! IAM adapter to provision a scoped user + policy + access-key, and writes
//! the resulting shared-backend row + `haex_shared_space_sync` mapping in a
//! single SQLite transaction.
//!
//! See `docs/plans/2026-07-04-s3-bucket-sharing-via-spaces-design.md` for
//! the end-to-end design.
//!
//! # v1 limitation
//!
//! Object-scoped shares (a single S3 key) are structurally rejected here
//! (`StorageError::ObjectScopeNotYetSupported`). The `haex_s3_backends`
//! schema only carries a `share_prefix` column today, with no way to
//! distinguish "prefix ending without slash" from "single object key" once
//! the row is round-tripped through CRDT sync. A follow-up task will add
//! a `share_scope_kind` column ("prefix" | "object") and lift the guard.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::critical::CriticalFailureCode;
use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::database::row::get_string;
use crate::remote_storage::error::StorageError;
use crate::remote_storage::iam_adapter::{
    AwsCompatIamAdapter, IamAdapter, IamAdapterError, ProviderFlavor,
};
use crate::remote_storage::iam_admin_creds::{self, IamAdminCred};
use crate::remote_storage::iam_policy::{build_object_policy, build_policy};
use crate::remote_storage::provider::ProviderKind;
use crate::table_names::{
    COL_S3_BACKENDS_CONFIG, COL_S3_BACKENDS_ID, COL_S3_BACKENDS_NAME, COL_S3_BACKENDS_TYPE,
    COL_SPACES_ID, COL_SPACES_NAME, TABLE_S3_BACKENDS, TABLE_SHARED_SPACE_SYNC, TABLE_SPACES,
};
use crate::AppState;

mod epoch_resolver;
mod shared_access_fanout;
pub use epoch_resolver::{DefaultEpochResolver, EpochResolver};
use shared_access_fanout::{rollback_child_backend_and_iam, write_shared_access_fanout};

/// Frontend-provided IAM-admin credential. Only populated on a retry after
/// the initial invocation returned `IamAdminCredMissing`; the vault stores it
/// via `iam_admin_creds::store` and then proceeds with the share flow.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IamAdminCredHint {
    pub access_key_id: String,
    pub secret_access_key: String,
    /// Provider identity. Unknown wire values fail at serde deserialisation
    /// and surface to the frontend as an argument error.
    pub provider_type: ProviderKind,
}

/// Arguments to `share_storage_backend`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareStorageBackendArgs {
    /// Owner-side `haex_s3_backends.id` to share.
    pub storage_id: String,
    /// Target space's id.
    pub space_id: String,
    /// Key-prefix scope. `None` = whole bucket.
    pub prefix: Option<String>,
    /// Object-key scope (single file). v1 rejects this variant — see
    /// [`StorageError::ObjectScopeNotYetSupported`].
    pub object_key: Option<String>,
    /// Bitmap over `share_access_flags::{LIST,GET,PUT,DELETE}`.
    /// Must be non-zero.
    pub access_flags: i64,
    /// Populated by the frontend on retry after `IamAdminCredMissing`.
    pub iam_admin_cred_hint: Option<IamAdminCredHint>,
}

/// The newly-written `haex_s3_backends` row as returned to the frontend.
/// Field set mirrors what the frontend already renders for other backends
/// so the shared row can be dropped straight into the existing list view.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedStorageBackend {
    pub id: String,
    pub r#type: String,
    pub name: String,
    /// New row's IAM user name — the frontend surfaces it so a subsequent
    /// revoke command can find the right user to tear down.
    pub iam_user_name: String,
}

/// Minimal view over the owner's `haex_s3_backends` row + its parsed JSON
/// config. Held on the stack for the duration of the share flow.
#[derive(Debug)]
struct OwnerBackend {
    r#type: String,
    name: String,
    /// Parsed JSON config from the `config` column. Includes bucket,
    /// endpoint, region, pathStyle (for S3-compat routing) plus the
    /// pre-existing access-key pair (kept for legacy readers — the share
    /// flow does not use those; it fetches its own IAM-admin cred via
    /// `iam_admin_creds`).
    config: serde_json::Value,
}

impl OwnerBackend {
    fn bucket(&self) -> Option<&str> {
        self.config.get("bucket").and_then(|v| v.as_str())
    }
}

/// Factory over `dyn IamAdapter` so unit tests can inject a mock without
/// hitting AWS/Wasabi. Production callers use the default variant which
/// constructs an [`AwsCompatIamAdapter`] from the loaded IAM-admin cred.
///
/// Trait-object-shaped rather than a plain `Fn` so an implementation can
/// hold state (test-time call recording, real reqwest client) without
/// gymnastics.
pub trait IamAdapterFactory: Send + Sync {
    fn build(
        &self,
        cred: &IamAdminCred,
        flavor: ProviderFlavor,
    ) -> Result<Arc<dyn IamAdapter>, IamAdapterError>;
}

/// Default factory: builds a real [`AwsCompatIamAdapter`] talking to
/// AWS / Wasabi.
pub struct DefaultIamAdapterFactory;

impl IamAdapterFactory for DefaultIamAdapterFactory {
    fn build(
        &self,
        cred: &IamAdminCred,
        flavor: ProviderFlavor,
    ) -> Result<Arc<dyn IamAdapter>, IamAdapterError> {
        let adapter =
            AwsCompatIamAdapter::new(&cred.access_key_id, &cred.secret_access_key, flavor)?;
        Ok(Arc::new(adapter))
    }
}

/// Map a loaded admin cred to the adapter's provider flavor. Shared with
/// `revoke_command` — both flows reject unsupported providers identically.
pub(crate) fn provider_flavor_from(cred: &IamAdminCred) -> Result<ProviderFlavor, StorageError> {
    cred.provider_type
        .to_flavor()
        .map_err(|e| StorageError::UnsupportedProvider {
            provider_type: format!("{}: {e}", cred.provider_type.to_slug()),
        })
}

/// Post-validation view over the request: trimmed prefix/object_key strings
/// live here so downstream code can use `&str` slices without re-trimming
/// (and so the debug_assert guards inside `iam_policy::build_policy` /
/// `build_object_policy` are never triggered by defense-in-depth trimming).
#[derive(Debug)]
struct ValidatedArgs {
    /// Trimmed of any trailing `/`. `None` = whole bucket.
    prefix: Option<String>,
    /// v1: always `None` — object-scope path is rejected upstream. Kept in
    /// the struct so the v2 wiring is a two-line change here.
    object_key: Option<String>,
    access_flags: i64,
}

fn validate_args(args: &ShareStorageBackendArgs) -> Result<ValidatedArgs, StorageError> {
    if args.access_flags == 0 {
        return Err(StorageError::InvalidArgs {
            reason: "accessFlags must have at least one bit set".to_string(),
        });
    }

    if args.prefix.is_some() && args.object_key.is_some() {
        return Err(StorageError::InvalidArgs {
            reason: "prefix and objectKey are mutually exclusive".to_string(),
        });
    }

    // v1 rejects object-scope. Structural check runs BEFORE the mutual-exclusion
    // path succeeds so the frontend gets the more informative error variant.
    if args.object_key.is_some() {
        return Err(StorageError::ObjectScopeNotYetSupported);
    }

    let prefix = args
        .prefix
        .as_ref()
        .map(|p| p.trim_end_matches('/').to_string())
        .filter(|p| !p.is_empty());

    // `*` and `?` are IAM wildcards in both the Resource ARN and the
    // `s3:prefix` StringLike condition. A folder literally named `logs*`
    // (legal as an S3 key) would silently broaden the policy beyond the
    // chosen scope, so reject rather than escape (IAM has no escaping).
    if let Some(p) = prefix.as_deref() {
        if p.contains('*') || p.contains('?') {
            return Err(StorageError::InvalidArgs {
                reason: "prefix must not contain the IAM wildcard characters '*' or '?'"
                    .to_string(),
            });
        }
    }

    Ok(ValidatedArgs {
        prefix,
        object_key: None,
        access_flags: args.access_flags,
    })
}

// ---------------------------------------------------------------------------
// DB reads
// ---------------------------------------------------------------------------

fn load_owner_backend(
    db: &crate::database::DbConnection,
    storage_id: &str,
) -> Result<OwnerBackend, StorageError> {
    let sql = format!(
        "SELECT {COL_S3_BACKENDS_TYPE}, {COL_S3_BACKENDS_NAME}, {COL_S3_BACKENDS_CONFIG} \
         FROM {TABLE_S3_BACKENDS} \
         WHERE {COL_S3_BACKENDS_ID} = ?1 AND origin_type = 'owned'"
    );
    let rows = select_with_crdt(
        sql,
        vec![serde_json::Value::String(storage_id.to_string())],
        db,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: e.to_string(),
    })?;

    let row = rows.first().ok_or_else(|| StorageError::StorageNotFound {
        storage_id: storage_id.to_string(),
    })?;

    let r#type = get_string(row, 0);
    let name = get_string(row, 1);
    let config_str = get_string(row, 2);
    let config: serde_json::Value =
        serde_json::from_str(&config_str).map_err(|e| StorageError::InvalidConfig {
            reason: format!("owner backend config JSON parse failed: {e}"),
        })?;

    Ok(OwnerBackend {
        r#type,
        name,
        config,
    })
}

/// Load the target space's `name`. Falls back to the raw space_id if the
/// space cannot be resolved — the shared row is still valid, only its
/// human-readable name becomes less descriptive. Never fails the share.
fn load_space_name(db: &crate::database::DbConnection, space_id: &str) -> String {
    let sql = format!("SELECT {COL_SPACES_NAME} FROM {TABLE_SPACES} WHERE {COL_SPACES_ID} = ?1");
    match select_with_crdt(
        sql,
        vec![serde_json::Value::String(space_id.to_string())],
        db,
    ) {
        Ok(rows) => rows
            .first()
            .map(|r| get_string(r, 0))
            .unwrap_or_else(|| space_id.to_string()),
        Err(_) => space_id.to_string(),
    }
}

/// GET-or-create dedupe: look up any existing shared-backend row for this
/// (parent_backend_id, space_id, share_prefix, share_access_flags) tuple.
///
/// Prevents double-provisioning when the user double-clicks Share — a real
/// AWS IAM user costs money and a duplicate row also duplicates the shared
/// space mapping. Match keys mirror the INSERT in `persist_shared_backend`:
/// `parent_backend_id` + `origin_type='shared_from_space'` + `share_prefix`
/// (NULL-safe via `IS NOT DISTINCT FROM`, emulated with the standard
/// `IFNULL` trick because SQLite lacks the operator) + `share_access_flags`
/// + `space_id` (via the mapping-row join on the JSON array's first entry).
///
/// Returns `Ok(None)` if no matching row exists — the fresh-provision path
/// then runs.
fn find_existing_share(
    db: &crate::database::DbConnection,
    parent_backend_id: &str,
    space_id: &str,
    share_prefix: Option<&str>,
    share_access_flags: i64,
) -> Result<Option<SharedStorageBackend>, StorageError> {
    // `row_pks` is stored as a JSON array with the shared-backend id at
    // index 0 (see `persist_shared_backend`). SQLite's `json_extract`
    // returns TEXT, which matches `haex_s3_backends.id` directly.
    let sql = format!(
        "SELECT b.id, b.type, b.name, b.config \
         FROM {TABLE_S3_BACKENDS} b \
         INNER JOIN {TABLE_SHARED_SPACE_SYNC} m \
           ON m.table_name = ?1 \
          AND m.space_id = ?2 \
          AND json_extract(m.row_pks, '$[0]') = b.id \
         WHERE b.parent_backend_id = ?3 \
           AND b.origin_type = 'shared_from_space' \
           AND b.share_access_flags = ?4 \
           AND IFNULL(b.share_prefix, '') = IFNULL(?5, '')"
    );
    let prefix_param = share_prefix
        .map(|p| serde_json::Value::String(p.to_string()))
        .unwrap_or(serde_json::Value::Null);

    let rows = select_with_crdt(
        sql,
        vec![
            serde_json::Value::String(TABLE_S3_BACKENDS.to_string()),
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(parent_backend_id.to_string()),
            serde_json::Value::Number(serde_json::Number::from(share_access_flags)),
            prefix_param,
        ],
        db,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: format!("find_existing_share: {e}"),
    })?;

    let Some(row) = rows.first() else {
        return Ok(None);
    };

    let id = get_string(row, 0);
    let r#type = get_string(row, 1);
    let name = get_string(row, 2);
    let config_str = get_string(row, 3);

    // Recover `iam_user_name` from the config JSON — the same field the
    // fresh-provision path writes on first success.
    let iam_user_name = serde_json::from_str::<serde_json::Value>(&config_str)
        .ok()
        .and_then(|v| {
            v.get("iamUserName")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();

    Ok(Some(SharedStorageBackend {
        id,
        r#type,
        name,
        iam_user_name,
    }))
}

// ---------------------------------------------------------------------------
// Tauri command entry point
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn share_storage_backend(
    state: State<'_, AppState>,
    args: ShareStorageBackendArgs,
) -> Result<SharedStorageBackend, StorageError> {
    // Snapshot the hlc guard once, hand a plain `&HlcService` into the
    // testable core so unit tests can bypass AppState.lock_or_fail
    // (which needs a critical_sink + real Tauri State) without losing
    // the poisoning check in production.
    let hlc_snapshot = {
        let guard = state.lock_or_fail(
            &state.hlc,
            CriticalFailureCode::HlcMutexPoisoned,
            "remote_storage::share_command::share_storage_backend",
            serde_json::json!({}),
        )?;
        guard.clone()
    };
    share_storage_backend_core(
        &state.db,
        &hlc_snapshot,
        &state.column_sig_key_cache,
        args,
        &DefaultIamAdapterFactory,
        &DefaultEpochResolver,
    )
    .await
}

/// Testable core form: takes the raw `db` + `hlc` so unit tests can call it
/// without spinning up a full `AppState`. Also parameterises the adapter
/// factory + epoch resolver so tests can inject mocks without hitting
/// AWS/Wasabi or seeding a real MLS group.
pub(crate) async fn share_storage_backend_core(
    db: &crate::database::DbConnection,
    hlc_service: &crate::crdt::hlc::HlcService,
    key_cache: &crate::crdt::column_sig::key_cache::SpaceKeyCache,
    args: ShareStorageBackendArgs,
    factory: &dyn IamAdapterFactory,
    epoch_resolver: &dyn EpochResolver,
) -> Result<SharedStorageBackend, StorageError> {
    // Serialise the whole share flow: the `find_existing_share` dedupe
    // check below only guards sequential repeats — two concurrent invokes
    // could both miss the row and double-provision a real IAM user (real
    // AWS $, plus a duplicate share row). Shares are a rare interactive
    // action, so one process-wide lock is proportionate; a per-storage-id
    // lock or DB unique constraint would be overkill (the share identity
    // spans two tables, so SQLite can't express it as one constraint).
    static SHARE_FLOW_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _flow_guard = SHARE_FLOW_LOCK.lock().await;

    let validated = validate_args(&args)?;

    let owner = load_owner_backend(db, &args.storage_id)?;
    let bucket = owner
        .bucket()
        .ok_or_else(|| StorageError::InvalidConfig {
            reason: "owner backend config missing 'bucket' field".to_string(),
        })?
        .to_string();

    // Idempotency: if a shared-backend row + mapping already exist for this
    // (parent, space, prefix, flags) tuple, return it unchanged. Guards
    // against double-clicks provisioning a second IAM user (real AWS $).
    //
    // Idempotent short-circuit invariant: this returns the existing
    // `SharedStorageBackend` WITHOUT re-running `write_shared_access_fanout`.
    // It assumes the `haex_s3_shared_access` rows minted at initial share are
    // still present. A housekeeping delete or an interrupted rollback that
    // removed them will NOT be repaired here — receivers will see the child
    // row but cannot resolve creds. Repair must go through a dedicated flow
    // (F5's rewrap orchestration).
    if let Some(existing) = find_existing_share(
        db,
        &args.storage_id,
        &args.space_id,
        validated.prefix.as_deref(),
        validated.access_flags,
    )? {
        return Ok(existing);
    }

    // Load or store-then-load the IAM-admin cred.
    let cred = obtain_iam_admin_cred(db, hlc_service, &args)?;

    let flavor = provider_flavor_from(&cred)?;
    let adapter = factory
        .build(&cred, flavor)
        .map_err(|e| StorageError::IamOperationFailed {
            operation: "build_adapter".to_string(),
            reason: e.to_string(),
        })?;

    // Probe: bail before we start creating IAM users if the cred lacks perms.
    match adapter.probe_iam_capability().await {
        Ok(true) => {}
        Ok(false) | Err(IamAdapterError::AccessDenied(_)) => {
            return Err(StorageError::IamAdminInsufficient);
        }
        Err(e) => {
            return Err(StorageError::IamOperationFailed {
                operation: "probe_iam_capability".to_string(),
                reason: e.to_string(),
            });
        }
    }

    // Build the IAM policy scoped as requested.
    let policy = if let Some(object_key) = validated.object_key.as_deref() {
        build_object_policy(&bucket, object_key, validated.access_flags)
    } else {
        build_policy(&bucket, validated.prefix.as_deref(), validated.access_flags)
    };

    // Provision the scoped user.
    let user_name = format!(
        "haex-share-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    );
    let scoped_cred = adapter
        .create_scoped_user(&user_name, &policy)
        .await
        .map_err(|e| StorageError::IamOperationFailed {
            operation: "create_scoped_user".to_string(),
            reason: e.to_string(),
        })?;

    // Compose the new backend row.
    let new_row_id = uuid::Uuid::new_v4().to_string();
    let space_name = load_space_name(db, &args.space_id);
    let new_name = format!("{} (Space {})", owner.name, space_name);

    let mut new_config = owner.config.clone();
    if let serde_json::Value::Object(ref mut obj) = new_config {
        // Phase 4 Round F3b: the scoped credential no longer lives in the
        // child backend row's config JSON. It is sealed under the space's
        // MLS epoch key and written to `haex_s3_shared_access` below;
        // receiving members read it from there and hand the plaintext to
        // the storage provider at request time. Strip any cred material
        // the owner row may carry so the child row emits ONLY structural
        // fields.
        for field in [
            "accessKeyId",
            "secretAccessKey",
            "sessionToken",
            "access_key_id",
            "secret_access_key",
            "session_token",
        ] {
            obj.remove(field);
        }
        // `iamUserName` is not a cred — revoke_share (future task) needs
        // it to find the IAM user to delete. Keep it in the child config.
        obj.insert(
            "iamUserName".to_string(),
            serde_json::Value::String(scoped_cred.iam_user_name.clone()),
        );
    } else {
        return Err(StorageError::InvalidConfig {
            reason: "owner backend config is not a JSON object".to_string(),
        });
    }
    let new_config_json =
        serde_json::to_string(&new_config).map_err(|e| StorageError::Internal {
            reason: format!("serialize new backend config: {e}"),
        })?;

    // Persist the row + shared-space-sync mapping. Errors here trigger a
    // best-effort IAM rollback.
    let db_result = persist_shared_backend(
        db,
        hlc_service,
        key_cache,
        PersistArgs {
            new_row_id: &new_row_id,
            row_type: &owner.r#type,
            row_name: &new_name,
            row_config_json: &new_config_json,
            parent_backend_id: &args.storage_id,
            share_prefix: validated.prefix.as_deref(),
            share_access_flags: validated.access_flags,
            space_id: &args.space_id,
        },
    );

    if let Err(db_err) = db_result {
        // The IAM user + policy + access-key already exist on the provider.
        // Best-effort rollback so we don't leave an orphan behind. Any
        // failure is logged; the DB error is what we surface, because
        // that's the actionable one.
        if let Err(rollback_err) = adapter.delete_scoped_user(&user_name).await {
            tracing::warn!(
                user_name = %user_name,
                db_error = %db_err,
                rollback_error = %rollback_err,
                "IAM rollback failed after DB insert failure; scoped user may be orphaned"
            );
        }
        return Err(db_err);
    }

    // Phase 4 Round F3b — hand the ScopedCred to receiving members via a
    // sealed row in `haex_s3_shared_access`. The seal is symmetric under
    // the space's MLS epoch key, so every current member unlocks with the
    // same key (which they resolve from `haex_mls_sync_keys` on their
    // side). We fan out one row per member so per-member revocation stays
    // possible: the UNIQUE (space, backend, member) constraint would
    // otherwise force one row shared across members.
    //
    // The seal + fanout runs AFTER persist_shared_backend because we need
    // the child backend id (`new_row_id`) to key the rows. If any step
    // here fails, we roll BOTH the shared_access fanout AND the child
    // backend row back — an orphan child row without matching sealed
    // creds would surface in the UI with no way for anyone to unlock it.
    if let Err(err) = write_shared_access_fanout(
        db,
        hlc_service,
        key_cache,
        epoch_resolver,
        &args.space_id,
        &new_row_id,
        &scoped_cred,
    ) {
        rollback_child_backend_and_iam(
            db,
            hlc_service,
            key_cache,
            adapter.as_ref(),
            &new_row_id,
            &args.space_id,
            &user_name,
        )
        .await;
        return Err(err);
    }

    Ok(SharedStorageBackend {
        id: new_row_id,
        r#type: owner.r#type,
        name: new_name,
        iam_user_name: scoped_cred.iam_user_name,
    })
}

/// Resolve the IAM-admin cred: either store the frontend-provided hint (if
/// any) and use that, or fall back to the previously-persisted entry. If
/// neither path yields a cred, return [`StorageError::IamAdminCredMissing`]
/// so the frontend can prompt the user.
fn obtain_iam_admin_cred(
    db: &crate::database::DbConnection,
    hlc_service: &crate::crdt::hlc::HlcService,
    args: &ShareStorageBackendArgs,
) -> Result<IamAdminCred, StorageError> {
    if let Some(hint) = args.iam_admin_cred_hint.as_ref() {
        // Validate the provider before we accept the hint — surfaces the
        // bad-input error before we perform an on-disk write we'd then
        // need to roll back. `ProviderKind` is already a closed enum
        // (unknown wire values fail at serde), so the only remaining
        // reject case is a variant we accept but can't drive yet (MinIO).
        if let Err(e) = hint.provider_type.to_flavor() {
            return Err(StorageError::UnsupportedProvider {
                provider_type: format!("{}: {e}", hint.provider_type.to_slug()),
            });
        }

        // iam_admin_creds::{store,delete_by_storage} require a
        // `&MutexGuard<HlcService>` (mirroring `execute_with_crdt`'s
        // signature), so wrap our owned HlcService in a local Mutex and
        // lock it for the duration of the two writes. The wrapping mutex
        // is exclusive to this call so contention is nil.
        let hlc_local = std::sync::Mutex::new(hlc_service.clone());
        let hlc_guard = hlc_local.lock().map_err(|e| StorageError::Internal {
            reason: format!("hlc local mutex poisoned: {e}"),
        })?;

        let cred = IamAdminCred {
            access_key_id: hint.access_key_id.clone(),
            secret_access_key: hint.secret_access_key.clone(),
            provider_type: hint.provider_type,
        };
        // Wipe any previous entry so retries after a bad cred don't leak
        // stale (accessKeyId → provider_type) pairings.
        iam_admin_creds::delete_by_storage(db, &hlc_guard, &args.storage_id).map_err(|e| {
            StorageError::DatabaseError {
                reason: format!("delete previous iam admin cred: {e}"),
            }
        })?;
        iam_admin_creds::store(db, &hlc_guard, &args.storage_id, &cred).map_err(|e| {
            StorageError::DatabaseError {
                reason: format!("store iam admin cred: {e}"),
            }
        })?;
        return Ok(cred);
    }

    match iam_admin_creds::load(db, &args.storage_id) {
        Ok(Some(cred)) => Ok(cred),
        Ok(None) => Err(StorageError::IamAdminCredMissing {
            storage_id: args.storage_id.clone(),
        }),
        Err(e) => Err(StorageError::DatabaseError {
            reason: format!("load iam admin cred: {e}"),
        }),
    }
}

/// Grouped parameters for [`persist_shared_backend`] — keeps the signature
/// readable in the face of clippy's argument-count lint.
struct PersistArgs<'a> {
    new_row_id: &'a str,
    row_type: &'a str,
    row_name: &'a str,
    row_config_json: &'a str,
    parent_backend_id: &'a str,
    share_prefix: Option<&'a str>,
    share_access_flags: i64,
    space_id: &'a str,
}

/// Insert the new `haex_s3_backends` row and the corresponding
/// `haex_shared_space_sync` mapping row. Both writes go through
/// `execute_with_crdt` — they land in separate SQLite transactions per
/// [`crate::database::core::execute_with_crdt`]'s per-call invariant.
///
/// # No-orphan invariant
///
/// If the s3_backends INSERT succeeds but the mapping INSERT then fails,
/// the freshly-inserted s3_backends row is best-effort deleted before the
/// error propagates. This keeps the DB free of "shared backend not attached
/// to any space" ghost rows, which would surface in the frontend list view
/// with no way to detach them.
///
/// If the cleanup DELETE itself fails (unlikely — same connection, same
/// transaction-context), a `tracing::error!` is emitted so an operator can
/// find the stale row, and the original mapping-insert error is still
/// returned unchanged.
fn persist_shared_backend(
    db: &crate::database::DbConnection,
    hlc_service: &crate::crdt::hlc::HlcService,
    key_cache: &crate::crdt::column_sig::key_cache::SpaceKeyCache,
    args: PersistArgs<'_>,
) -> Result<(), StorageError> {
    let hlc_local = std::sync::Mutex::new(hlc_service.clone());
    let hlc_guard = hlc_local.lock().map_err(|e| StorageError::Internal {
        reason: format!("hlc local mutex poisoned: {e}"),
    })?;

    // 1. Insert into haex_s3_backends. `origin_type = 'shared_from_space'`,
    //    plus the parent id + share scope so revoke_share can find the
    //    right IAM user later.
    let insert_backend = format!(
        "INSERT INTO {TABLE_S3_BACKENDS} \
         (id, type, name, config, enabled, parent_backend_id, origin_type, \
          share_prefix, share_access_flags) \
         VALUES (?1, ?2, ?3, ?4, 1, ?5, 'shared_from_space', ?6, ?7)"
    );
    execute_with_crdt(
        insert_backend,
        vec![
            serde_json::Value::String(args.new_row_id.to_string()),
            serde_json::Value::String(args.row_type.to_string()),
            serde_json::Value::String(args.row_name.to_string()),
            serde_json::Value::String(args.row_config_json.to_string()),
            serde_json::Value::String(args.parent_backend_id.to_string()),
            args.share_prefix
                .map(|s| serde_json::Value::String(s.to_string()))
                .unwrap_or(serde_json::Value::Null),
            serde_json::Value::Number(serde_json::Number::from(args.share_access_flags)),
        ],
        db,
        &hlc_guard,
        key_cache,
    )
    .map_err(|e| StorageError::DatabaseError {
        reason: format!("insert haex_s3_backends: {e}"),
    })?;

    // 2. Insert the haex_shared_space_sync mapping so the sync engine picks
    //    the row up. rowPks is a JSON array (one entry: the backend id).
    //    extension_public_key + extension_name intentionally NULL —
    //    this row is user-owned, not extension-owned. The schema's CHECK
    //    constraint permits both NULL together.
    let row_pks_json =
        serde_json::to_string(&vec![args.new_row_id]).map_err(|e| StorageError::Internal {
            reason: format!("serialize row_pks: {e}"),
        })?;
    let mapping_id = uuid::Uuid::new_v4().to_string();
    let insert_mapping = format!(
        "INSERT INTO {TABLE_SHARED_SPACE_SYNC} \
         (id, table_name, row_pks, space_id, extension_public_key, extension_name, \
          category, type, type_label) \
         VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, ?5, ?6)"
    );
    if let Err(map_err) = execute_with_crdt(
        insert_mapping,
        vec![
            serde_json::Value::String(mapping_id),
            serde_json::Value::String(TABLE_S3_BACKENDS.to_string()),
            serde_json::Value::String(row_pks_json),
            serde_json::Value::String(args.space_id.to_string()),
            serde_json::Value::String("cloud_storage".to_string()),
            serde_json::Value::String(args.row_name.to_string()),
        ],
        db,
        &hlc_guard,
        key_cache,
    ) {
        // Orphan cleanup: the s3_backends row is already committed to its
        // own tx, so a naive DELETE via execute_with_crdt is our only
        // option. Log & swallow a failed cleanup so the caller's
        // best-effort IAM rollback still fires downstream.
        let delete_orphan = format!("DELETE FROM {TABLE_S3_BACKENDS} WHERE id = ?1");
        if let Err(cleanup_err) = execute_with_crdt(
            delete_orphan,
            vec![serde_json::Value::String(args.new_row_id.to_string())],
            db,
            &hlc_guard,
            key_cache,
        ) {
            tracing::error!(
                storage_id = %args.new_row_id,
                mapping_error = %map_err,
                cleanup_error = %cleanup_err,
                "failed to clean up orphan s3_backends row after mapping-insert \
                 failure; DB now has an orphan share row"
            );
        }
        return Err(StorageError::DatabaseError {
            reason: format!("insert haex_shared_space_sync: {map_err}"),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests;

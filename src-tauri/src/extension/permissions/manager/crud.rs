use super::PermissionManager;
use crate::database::core::{select_with_crdt, with_connection};
use crate::database::error::DatabaseError;
use crate::database::generated::HaexPrincipalPermissions;
use crate::extension::core::types::Extension;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::types::{
    split_constraints, Action, ExtensionPermission, PermissionStatus, Principal, ResourceType,
};
use crate::table_names::TABLE_PRINCIPAL_PERMISSIONS;
use crate::AppState;
use rusqlite::params;
use serde_json::Value as JsonValue;
use tauri::State;

impl PermissionManager {
    /// Speichert alle Permissions einer Extension
    pub async fn save_permissions(
        app_state: &State<'_, AppState>,
        permissions: &[ExtensionPermission],
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            let sql = format!(
                "INSERT INTO {TABLE_PRINCIPAL_PERMISSIONS} (id, principal_id, resource_type, action, target, constraints, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
            );

            for perm in permissions {
                // 1. Konvertiere App-Struct zu DB-Struct
                let db_perm: HaexPrincipalPermissions = perm.into();

                // 2. Erstelle typsichere Parameter
                let params = params![
                    db_perm.id,
                    db_perm.principal_id,
                    db_perm.resource_type,
                    db_perm.action,
                    db_perm.target,
                    db_perm.constraints,
                    db_perm.status,
                ];

                // 3. Führe mit dem typsicheren Executor aus
                SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params)?;
            }

            tx.commit().map_err(DatabaseError::from)?;
            Ok(())
        })
        .map_err(ExtensionError::from)
    }

    /// Aktualisiert eine Permission
    #[allow(dead_code)]
    pub async fn update_permission(
        app_state: &State<'_, AppState>,
        permission: &ExtensionPermission,
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            let db_perm: HaexPrincipalPermissions = permission.into();

            let sql = format!(
                "UPDATE {TABLE_PRINCIPAL_PERMISSIONS} SET resource_type = ?, action = ?, target = ?, constraints = ?, status = ? WHERE id = ?"
            );

            let params = params![
                db_perm.resource_type,
                db_perm.action,
                db_perm.target,
                db_perm.constraints,
                db_perm.status,
                db_perm.id,
            ];

            SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params)?;
            tx.commit().map_err(DatabaseError::from)
        })
        .map_err(ExtensionError::from)
    }

    /// Ändert den Status einer Permission
    pub async fn update_permission_status(
        app_state: &State<'_, AppState>,
        permission_id: &str,
        new_status: PermissionStatus,
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            let sql = format!("UPDATE {TABLE_PRINCIPAL_PERMISSIONS} SET status = ? WHERE id = ?");
            let params = params![new_status.as_str(), permission_id];
            SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params)?;
            tx.commit().map_err(DatabaseError::from)
        })
        .map_err(ExtensionError::from)
    }

    /// Löscht alle Permissions einer Extension
    #[allow(dead_code)]
    pub async fn delete_permission(
        app_state: &State<'_, AppState>,
        permission_id: &str,
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            // Echtes DELETE - wird vom CrdtTransformer zu UPDATE umgewandelt
            let sql = format!("DELETE FROM {TABLE_PRINCIPAL_PERMISSIONS} WHERE id = ?");
            SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params![permission_id])?;
            tx.commit().map_err(DatabaseError::from)
        })
        .map_err(ExtensionError::from)
    }

    /// Löscht alle Permissions einer Extension (Soft-Delete)
    pub async fn delete_permissions(
        app_state: &State<'_, AppState>,
        extension_id: &str,
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            let sql = format!("DELETE FROM {TABLE_PRINCIPAL_PERMISSIONS} WHERE principal_id = ?");
            SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params![extension_id])?;
            tx.commit().map_err(DatabaseError::from)
        })
        .map_err(ExtensionError::from)
    }

    /// Löscht alle Permissions einer Extension innerhalb einer bestehenden Transaktion
    pub fn delete_permissions_in_transaction(
        tx: &rusqlite::Transaction,
        hlc_service: &crate::crdt::hlc::HlcService,
        extension_id: &str,
    ) -> Result<(), DatabaseError> {
        let sql = format!("DELETE FROM {TABLE_PRINCIPAL_PERMISSIONS} WHERE principal_id = ?");
        SqlExecutor::execute_internal_typed(tx, hlc_service, &sql, params![extension_id])?;
        Ok(())
    }

    /// Atomically replace all permissions for an extension within a single transaction.
    ///
    /// Deletes the extension's existing permissions and inserts the new set in
    /// ONE database transaction. If any insert fails the transaction is dropped
    /// without commit, so the original rows survive — callers cannot end up
    /// with an extension that has zero permissions after a partial save.
    ///
    /// Replaces the previous pattern `delete_permissions(..).await; save_permissions(..).await`,
    /// which used two independent transactions and could leave permissions in
    /// an empty state if the second call failed.
    pub async fn replace_permissions(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        permissions: &[ExtensionPermission],
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            Self::delete_permissions_in_transaction(&tx, &hlc_service, extension_id)?;

            let sql = format!(
                "INSERT INTO {TABLE_PRINCIPAL_PERMISSIONS} (id, principal_id, resource_type, action, target, constraints, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
            );

            for perm in permissions {
                let db_perm: HaexPrincipalPermissions = perm.into();
                let params = params![
                    db_perm.id,
                    db_perm.principal_id,
                    db_perm.resource_type,
                    db_perm.action,
                    db_perm.target,
                    db_perm.constraints,
                    db_perm.status,
                ];
                SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params)?;
            }

            tx.commit().map_err(DatabaseError::from)?;
            Ok(())
        })
        .map_err(ExtensionError::from)
    }
    /// Lädt alle Permissions einer Extension
    /// Uses select_with_crdt to automatically filter out tombstoned (soft-deleted) entries
    pub async fn get_permissions(
        app_state: &State<'_, AppState>,
        principal: &Principal,
    ) -> Result<Vec<ExtensionPermission>, ExtensionError> {
        let sql = format!(
            "SELECT id, principal_id, resource_type, action, target, constraints, status, haex_hlc FROM {TABLE_PRINCIPAL_PERMISSIONS} WHERE principal_id = ?"
        );
        let params = vec![JsonValue::String(principal.id().to_string())];

        let results = select_with_crdt(sql, params, &app_state.db)?;

        let permissions = results
            .into_iter()
            .map(|row| {
                let resource_type = row[2]
                    .as_str()
                    .and_then(|s| ResourceType::from_str(s).ok())
                    .unwrap_or(ResourceType::Db);
                let action = row[3]
                    .as_str()
                    .and_then(|s| Action::from_str(&resource_type, s).ok())
                    .unwrap_or(Action::Database(
                        crate::extension::permissions::types::DbAction::Read,
                    ));
                let mut status = row[6]
                    .as_str()
                    .and_then(|s| PermissionStatus::from_str(s).ok())
                    .unwrap_or(PermissionStatus::Denied);
                // Passwords keep their free-form `{"default":true}` constraint
                // raw (the typed untagged enum can't represent it); all other
                // resource types parse into the typed enum. The invariant lives
                // in `split_constraints` (single source of truth) — this is the
                // live `check_passwords_permission` read path.
                //
                // Fail closed: malformed JSON in the `constraints` column used
                // to be silently dropped to `(None, None)`, which downstream
                // matchers treat as "no constraints" and therefore *grant*
                // whatever the row's (resource_type, target, action) covers.
                // Force the row to `Denied` so deny-first precedence makes the
                // request fail rather than fail-open.
                let id_for_log = row[0].as_str().unwrap_or_default().to_string();
                let principal_id_for_log = row[1].as_str().unwrap_or_default().to_string();
                let target_for_log = row[4].as_str().unwrap_or_default().to_string();
                let (constraints, raw_constraints) =
                    match split_constraints(resource_type, row[5].as_str()) {
                        Ok(pair) => pair,
                        Err(err) => {
                            eprintln!(
                                "[permissions] malformed constraints JSON on permission id={} principal_id={} resource_type={} target={:?} — forcing status=Denied (parse error: {})",
                                id_for_log,
                                principal_id_for_log,
                                resource_type.as_str(),
                                target_for_log,
                                err
                            );
                            status = PermissionStatus::Denied;
                            (None, None)
                        }
                    };

                ExtensionPermission {
                    id: id_for_log,
                    principal_id: principal_id_for_log,
                    resource_type,
                    action,
                    target: target_for_log,
                    constraints,
                    status,
                    raw_constraints,
                }
            })
            .collect();

        Ok(permissions)
    }

    /// Load the calling extension and its persisted permissions in one go.
    ///
    /// Every `check_*_permission` method opens with the same prelude: look up
    /// the extension by id (rejecting an unknown principal with a validation
    /// error) and fetch its principal-permission rows. Centralising it here
    /// keeps that contract in one place and lets each `check_*` method focus
    /// on its resource-specific matching logic instead of repeating the
    /// lookup boilerplate.
    ///
    /// `extension_manager.get_extension` already returns the extension by
    /// value, so the per-call-site `.clone()` that used to follow this block
    /// was redundant and is gone here too.
    pub(super) async fn load_extension_and_permissions(
        app_state: &State<'_, AppState>,
        principal: &Principal,
    ) -> Result<(Extension, Vec<ExtensionPermission>), ExtensionError> {
        let extension_id = principal.id();
        let extension = app_state
            .extension_manager
            .get_extension(extension_id)
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: format!("Extension with ID {extension_id} not found"),
            })?;
        let permissions = Self::get_permissions(app_state, principal).await?;
        Ok((extension, permissions))
    }

    /// Load a principal's display name and persisted permission rows in one go.
    ///
    /// Generalizes `load_extension_and_permissions` to also cover
    /// `Principal::ExternalClient` — external bridge clients have no
    /// `Extension`/manifest, so there is no `extension.manifest.name` to read.
    /// Their display name instead comes from
    /// `haex_external_authorized_clients_no_sync.client_name`, falling back to
    /// the in-memory session authorization ("allow once" clients have no DB
    /// row) and finally to the raw client_id.
    pub(super) async fn load_principal_display_name_and_permissions(
        app_state: &State<'_, AppState>,
        principal: &Principal,
    ) -> Result<(String, Vec<ExtensionPermission>), ExtensionError> {
        match principal {
            Principal::Extension(_) => {
                let (extension, permissions) =
                    Self::load_extension_and_permissions(app_state, principal).await?;
                Ok((extension.manifest.name.clone(), permissions))
            }
            Principal::ExternalClient(client_id) => {
                let display_name = match Self::lookup_client_display_name(app_state, client_id) {
                    Some(name) => name,
                    None => {
                        // "Allow once" clients have no authorized-client row —
                        // fall back to the session authorization's name before
                        // resorting to the raw client id. The bridge (and its
                        // session store) only exists on desktop builds; mobile
                        // has no external clients, so the raw id suffices.
                        #[cfg(not(any(target_os = "android", target_os = "ios")))]
                        {
                            let bridge = app_state.external_bridge.lock().await;
                            bridge
                                .get_session_authorization(client_id)
                                .await
                                .map(|sa| sa.client_name)
                                .unwrap_or_else(|| client_id.clone())
                        }
                        #[cfg(any(target_os = "android", target_os = "ios"))]
                        {
                            client_id.clone()
                        }
                    }
                };
                let permissions = Self::get_permissions(app_state, principal).await?;
                Ok((display_name, permissions))
            }
        }
    }

    /// Best-effort lookup of an external client's human-readable name.
    /// `None` (not an error) when the client has no authorized-client row —
    /// callers fall back to the raw client_id.
    fn lookup_client_display_name(
        app_state: &State<'_, AppState>,
        client_id: &str,
    ) -> Option<String> {
        use crate::table_names::{
            COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID, COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_NAME,
            TABLE_EXTERNAL_AUTHORIZED_CLIENTS,
        };
        let sql = format!(
            "SELECT {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_NAME} FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS} \
             WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1 LIMIT 1"
        );
        let rows = select_with_crdt(
            sql,
            vec![JsonValue::String(client_id.to_string())],
            &app_state.db,
        )
        .ok()?;
        rows.first()?.first()?.as_str().map(|s| s.to_string())
    }
}

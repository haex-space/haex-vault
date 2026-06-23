use crate::extension::error::ExtensionError;
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::manager::check::deny_first_precedence;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, PermissionStatus, Principal, ResourceType,
};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Prüft Datenbankberechtigungen
    /// Returns PermissionPromptRequired if status is Ask or no permission exists
    /// Returns PermissionDenied if status is explicitly Denied
    pub async fn check_database_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: Action,
        table_name: &str,
    ) -> Result<(), ExtensionError> {
        let extension_id = principal.id();

        // Extract DbAction from Action enum
        let db_action = match action {
            Action::Database(db_action) => db_action,
            _ => {
                return Err(ExtensionError::ValidationError {
                    reason: "Expected database action".to_string(),
                });
            }
        };

        let (extension, permissions) =
            Self::load_extension_and_permissions(app_state, principal).await?;

        // Create checker and validate
        let checker = PermissionChecker::new(extension.clone(), permissions.clone());

        // First check if auto-allowed (extension's own tables).
        // External clients have no own DB tables, so this auto-allow path is
        // extension-only. Today every principal is an extension, so this is
        // behavior-preserving.
        if principal.is_extension() && checker.is_auto_allowed_table(table_name) {
            return Ok(());
        }

        // Resolve matching permissions deny-first, so an explicit `Denied` row
        // can never be hidden behind a `Granted` row regardless of insertion
        // order. See `deny_first_precedence` for the precedence rules.
        let resolved = database_matching_status(&checker, &permissions, table_name, db_action);

        match resolved {
            Some(PermissionStatus::Granted) => Ok(()),
            Some(PermissionStatus::Denied) => Err(ExtensionError::permission_denied(
                extension_id,
                db_action.as_str(),
                &format!("database table '{table_name}'"),
            )),
            Some(PermissionStatus::Ask) => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "db",
                db_action.as_str(),
                table_name,
            )),
            // No matching permission in database - check session permissions
            None => {
                if app_state.session_permissions.is_granted(
                    extension_id,
                    &Action::Database(db_action),
                    ResourceType::Db,
                    table_name,
                ) {
                    return Ok(());
                }
                if app_state.session_permissions.is_denied(
                    extension_id,
                    &Action::Database(db_action),
                    ResourceType::Db,
                    table_name,
                ) {
                    return Err(ExtensionError::permission_denied(
                        extension_id,
                        db_action.as_str(),
                        &format!("database table '{table_name}'"),
                    ));
                }

                // No session permission either - prompt the user
                Err(ExtensionError::permission_prompt_required(
                    extension_id,
                    &extension.manifest.name,
                    "db",
                    db_action.as_str(),
                    table_name,
                ))
            }
        }
    }
}

/// Resolves the matching DB permission status for `(table_name, db_action)` with
/// **deny-first precedence**. Returns `None` when no DB row matches.
///
/// Pure helper (no `State<AppState>`) so the security-critical action+target
/// matching and deny-wins precedence are unit-testable, mirroring
/// `identities_matching_status`.
pub(crate) fn database_matching_status(
    checker: &PermissionChecker,
    permissions: &[ExtensionPermission],
    table_name: &str,
    db_action: DbAction,
) -> Option<PermissionStatus> {
    deny_first_precedence(
        permissions
            .iter()
            .filter(|perm| {
                perm.resource_type == ResourceType::Db
                    && checker.matches_table_pattern(&perm.target, table_name)
                    && checker.action_allows_db_action(&perm.action, db_action)
            })
            .map(|perm| perm.status),
    )
}

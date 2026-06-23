use crate::database::core::select_with_crdt;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::check::deny_first_precedence;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, PermissionConstraints, PermissionStatus, Principal, ResourceType,
    WebAction,
};
use crate::table_names::TABLE_PRINCIPAL_PERMISSIONS;
use crate::AppState;
use serde_json::Value as JsonValue;
use tauri::State;

impl PermissionManager {
    /// Prüft Web-Berechtigungen für Requests
    /// Method/operation is not checked - only protocol, domain, port, and path
    /// Returns PermissionPromptRequired if status is Ask or no permission exists
    /// Returns PermissionDenied if status is explicitly Denied
    pub async fn check_web_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        url: &str,
    ) -> Result<(), ExtensionError> {
        let extension_id = principal.id();

        // Get extension for name lookup
        let extension = app_state
            .extension_manager
            .get_extension(extension_id)
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: format!("Extension not found: {}", extension_id),
            })?
            .clone();

        // Load permissions from database (same for dev and production extensions)
        let sql = format!(
            "SELECT id, principal_id, resource_type, action, target, constraints, status, haex_hlc FROM {TABLE_PRINCIPAL_PERMISSIONS} WHERE principal_id = ? AND resource_type = 'web'"
        );
        let params = vec![JsonValue::String(extension_id.to_string())];

        let results = select_with_crdt(sql, params, &app_state.db)?;

        let permissions: Vec<ExtensionPermission> = results
            .into_iter()
            .map(|row| {
                let resource_type = row[2]
                    .as_str()
                    .and_then(|s| ResourceType::from_str(s).ok())
                    .unwrap_or(ResourceType::Web);
                let action = row[3]
                    .as_str()
                    .and_then(|s| Action::from_str(&resource_type, s).ok())
                    .unwrap_or(Action::Web(
                        crate::extension::permissions::types::WebAction::Get,
                    ));
                let status = row[6]
                    .as_str()
                    .and_then(|s| PermissionStatus::from_str(s).ok())
                    .unwrap_or(PermissionStatus::Denied);
                let constraints: Option<PermissionConstraints> =
                    row[5].as_str().and_then(|s| serde_json::from_str(s).ok());

                ExtensionPermission {
                    id: row[0].as_str().unwrap_or_default().to_string(),
                    principal_id: row[1].as_str().unwrap_or_default().to_string(),
                    resource_type,
                    action,
                    target: row[4].as_str().unwrap_or_default().to_string(),
                    constraints,
                    status,
                    // web-only loader (WHERE resource_type = 'web'); never passwords
                    raw_constraints: None,
                }
            })
            .collect();

        let url_parsed = url::Url::parse(url).map_err(|e| ExtensionError::ValidationError {
            reason: format!("Invalid URL: {}", e),
        })?;

        let domain = url_parsed
            .host_str()
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: "URL does not contain a valid host".to_string(),
            })?;

        // Find matching permission status for this URL (deny-first precedence).
        match web_matching_status(&permissions, url, domain) {
            Some(PermissionStatus::Granted) => Ok(()),
            Some(PermissionStatus::Denied) => Err(ExtensionError::permission_denied(
                extension_id,
                "web request",
                url,
            )),
            Some(PermissionStatus::Ask) => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "web",
                "request",
                url,
            )),
            // No matching permission in database - check session permissions
            None => {
                if app_state.session_permissions.is_granted(
                    extension_id,
                    &Action::Web(WebAction::All),
                    ResourceType::Web,
                    url,
                ) {
                    return Ok(());
                }
                if app_state.session_permissions.is_denied(
                    extension_id,
                    &Action::Web(WebAction::All),
                    ResourceType::Web,
                    url,
                ) {
                    return Err(ExtensionError::permission_denied(
                        extension_id,
                        "web request",
                        url,
                    ));
                }

                // No session permission either - prompt the user
                Err(ExtensionError::permission_prompt_required(
                    extension_id,
                    &extension.manifest.name,
                    "web",
                    "request",
                    url,
                ))
            }
        }
    }
}

/// Resolves the matching web permission status for `(url, domain)` with
/// **deny-first precedence**. Returns `None` when no web row matches.
///
/// Pure helper (no `State<AppState>`) so the security-critical URL/domain
/// matching and deny-wins precedence are unit-testable, mirroring
/// `database_matching_status` and `filesystem_matching_status`.
///
/// The matching predicate is byte-identical to the pre-refactor
/// `iter().find()` body: `*` is a universal wildcard, targets containing
/// `://` are URL patterns, and bare targets match by domain (exact or
/// suffix-`.target`).
pub(crate) fn web_matching_status(
    permissions: &[ExtensionPermission],
    url: &str,
    domain: &str,
) -> Option<PermissionStatus> {
    deny_first_precedence(
        permissions
            .iter()
            .filter(|perm| {
                perm.resource_type == ResourceType::Web && {
                    if perm.target == "*" {
                        true
                    } else if perm.target.contains("://") {
                        PermissionManager::matches_url_pattern(&perm.target, url)
                    } else {
                        perm.target == domain || domain.ends_with(&format!(".{}", perm.target))
                    }
                }
            })
            .map(|perm| perm.status),
    )
}

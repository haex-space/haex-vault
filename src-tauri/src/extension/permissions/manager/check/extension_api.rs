use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::check::deny_first_precedence;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, ExtensionApiAction, ExtensionPermission, PermissionStatus, Principal, ResourceType,
};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Checks whether `principal` (an external bridge client) may call
    /// `action_name` on the extension identified by
    /// `(extension_public_key, extension_name)`.
    ///
    /// Target matching (exact first, then broader wildcards):
    /// - `"{pk}::{name}::{action_name}"` — exact action grant/deny
    /// - `"{pk}::{name}::*"` — all actions on this one extension
    /// - `"*"` — all actions on all extensions
    ///
    /// Deny-first precedence across matches, mirroring `check_database_permission`.
    /// `None` (no matching DB row at all) falls back to the session store,
    /// then prompts; an explicit `Ask` DB row prompts directly.
    pub async fn check_extension_api_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        extension_public_key: &str,
        extension_name: &str,
        action_name: &str,
    ) -> Result<(), ExtensionError> {
        let client_id = principal.id();
        let (display_name, permissions) =
            Self::load_principal_display_name_and_permissions(app_state, principal).await?;

        let exact_target = format!("{extension_public_key}::{extension_name}::{action_name}");
        let wildcard_target = format!("{extension_public_key}::{extension_name}::*");

        let resolved = extension_api_matching_status(&permissions, &exact_target, &wildcard_target);

        match resolved {
            Some(PermissionStatus::Granted) => Ok(()),
            Some(PermissionStatus::Denied) => Err(ExtensionError::permission_denied(
                client_id,
                "call",
                &exact_target,
            )),
            Some(PermissionStatus::Ask) => Err(ExtensionError::permission_prompt_required(
                client_id,
                &display_name,
                "extensionApi",
                "call",
                &exact_target,
            )),
            None => {
                // Session ("allow once") grants are stored under the exact
                // target when they come from a runtime prompt, but the
                // authorization-dialog path stores the client's declared
                // targets — which may be the extension wildcard (`…::*`,
                // declared `actions: ["*"]`). Resolve exact + both wildcard
                // targets with the same deny-first precedence as the
                // persisted rows above.
                let action = Action::ExtensionApi(ExtensionApiAction::Call);
                let session_resolved = deny_first_precedence(
                    [exact_target.as_str(), wildcard_target.as_str(), "*"]
                        .into_iter()
                        .filter_map(|target| {
                            app_state.session_permissions.get_permission(
                                client_id,
                                &action,
                                ResourceType::ExtensionApi,
                                target,
                            )
                        }),
                );

                match session_resolved {
                    Some(PermissionStatus::Granted) => Ok(()),
                    Some(PermissionStatus::Denied) => Err(ExtensionError::permission_denied(
                        client_id,
                        "call",
                        &exact_target,
                    )),
                    _ => Err(ExtensionError::permission_prompt_required(
                        client_id,
                        &display_name,
                        "extensionApi",
                        "call",
                        &exact_target,
                    )),
                }
            }
        }
    }
}

/// Resolves the matching `ExtensionApi` permission status with deny-first
/// precedence. Pure helper (no `State<AppState>`) so the target-matching
/// logic is unit-testable, mirroring `database_matching_status`.
pub(crate) fn extension_api_matching_status(
    permissions: &[ExtensionPermission],
    exact_target: &str,
    wildcard_target: &str,
) -> Option<PermissionStatus> {
    deny_first_precedence(
        permissions
            .iter()
            .filter(|p| {
                p.resource_type == ResourceType::ExtensionApi
                    && matches!(p.action, Action::ExtensionApi(ExtensionApiAction::Call))
                    && (p.target == exact_target || p.target == wildcard_target || p.target == "*")
            })
            .map(|p| p.status),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perm(target: &str, status: PermissionStatus) -> ExtensionPermission {
        ExtensionPermission {
            id: "id".to_string(),
            principal_id: "client-1".to_string(),
            resource_type: ResourceType::ExtensionApi,
            action: Action::ExtensionApi(ExtensionApiAction::Call),
            target: target.to_string(),
            constraints: None,
            status,
            raw_constraints: None,
        }
    }

    const EXACT: &str = "pk::haex-notes::getItems";
    const WILDCARD: &str = "pk::haex-notes::*";

    #[test]
    fn no_rows_returns_none() {
        assert_eq!(extension_api_matching_status(&[], EXACT, WILDCARD), None);
    }

    #[test]
    fn exact_match_wins() {
        let perms = vec![perm(EXACT, PermissionStatus::Granted)];
        assert_eq!(
            extension_api_matching_status(&perms, EXACT, WILDCARD),
            Some(PermissionStatus::Granted)
        );
    }

    #[test]
    fn extension_wildcard_matches_any_action_on_that_extension() {
        let perms = vec![perm(WILDCARD, PermissionStatus::Granted)];
        assert_eq!(
            extension_api_matching_status(&perms, EXACT, WILDCARD),
            Some(PermissionStatus::Granted)
        );
    }

    #[test]
    fn global_wildcard_matches_any_extension_any_action() {
        let perms = vec![perm("*", PermissionStatus::Granted)];
        assert_eq!(
            extension_api_matching_status(&perms, EXACT, WILDCARD),
            Some(PermissionStatus::Granted)
        );
    }

    #[test]
    fn unrelated_target_does_not_match() {
        let perms = vec![perm("pk::other-ext::getItems", PermissionStatus::Granted)];
        assert_eq!(extension_api_matching_status(&perms, EXACT, WILDCARD), None);
    }

    #[test]
    fn deny_wins_over_grant_regardless_of_order() {
        let perms = vec![
            perm(WILDCARD, PermissionStatus::Granted),
            perm(EXACT, PermissionStatus::Denied),
        ];
        assert_eq!(
            extension_api_matching_status(&perms, EXACT, WILDCARD),
            Some(PermissionStatus::Denied)
        );
    }

    #[test]
    fn ask_is_returned_when_no_grant_or_deny_present() {
        let perms = vec![perm(EXACT, PermissionStatus::Ask)];
        assert_eq!(
            extension_api_matching_status(&perms, EXACT, WILDCARD),
            Some(PermissionStatus::Ask)
        );
    }

    #[test]
    fn wrong_resource_type_is_ignored() {
        let mut p = perm(EXACT, PermissionStatus::Granted);
        p.resource_type = ResourceType::Db;
        assert_eq!(extension_api_matching_status(&[p], EXACT, WILDCARD), None);
    }
}

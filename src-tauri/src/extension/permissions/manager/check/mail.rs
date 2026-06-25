use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, MailAction, PermissionStatus, Principal, ResourceType,
};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Prüft Mail-Berechtigungen für IMAP-Fetch oder SMTP-Send.
    ///
    /// `host` ist der Mailserver-Hostname (z.B. "imap.gmail.com"). Matching:
    /// - target="*" → Wildcard, gewährt für jeden Host
    /// - target="imap.gmail.com" → exakter Hostname-Match
    /// - target="gmail.com" → matched "imap.gmail.com" und "smtp.gmail.com"
    ///   (Subdomain-Match)
    pub async fn check_mail_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: MailAction,
        host: &str,
    ) -> Result<(), ExtensionError> {
        let extension_id = principal.id();

        let (extension, permissions) =
            Self::load_extension_and_permissions(app_state, principal).await?;

        let action_matches = |perm_action: &Action| -> bool {
            matches!(perm_action, Action::Mail(a) if *a == action)
        };

        // Mail hostnames are case-insensitive (DNS), so we compare lowercased.
        let host_lower = host.to_ascii_lowercase();
        let host_matches = |target: &str| -> bool {
            let target_lower = target.to_ascii_lowercase();
            if target_lower == "*" {
                return true;
            }
            if target_lower == host_lower {
                return true;
            }
            // Subdomain match: target="gmail.com" matches "imap.gmail.com"
            host_lower.ends_with(&format!(".{}", target_lower))
        };

        let matching: Vec<&ExtensionPermission> = permissions
            .iter()
            .filter(|p| {
                p.resource_type == ResourceType::Mail
                    && action_matches(&p.action)
                    && host_matches(&p.target)
            })
            .collect();

        // Session permissions are keyed by exact target string, so a session
        // grant for "gmail.com" would not match a request for "imap.gmail.com"
        // through the store's own lookup. Apply the same `host_matches`
        // (and `action_matches`) logic against the extension's session
        // entries so behavior is consistent with DB-backed permissions.
        // Mirror the DB path's Denied-wins precedence: if any matching entry
        // is Denied, that overrides any other Granted entry (e.g. a wildcard
        // grant should not bypass a host-specific deny).
        let matching_session: Vec<ExtensionPermission> = app_state
            .session_permissions
            .get_permissions_for_extension(extension_id)
            .into_iter()
            .filter(|p| {
                p.resource_type == ResourceType::Mail
                    && action_matches(&p.action)
                    && host_matches(&p.target)
            })
            .collect();
        let session_status = if matching_session
            .iter()
            .any(|p| matches!(p.status, PermissionStatus::Denied))
        {
            Some(PermissionStatus::Denied)
        } else if matching_session
            .iter()
            .any(|p| matches!(p.status, PermissionStatus::Granted))
        {
            Some(PermissionStatus::Granted)
        } else {
            matching_session.into_iter().next().map(|p| p.status)
        };

        // Session-scoped grants (one-time prompt decisions) take priority
        // over the absence of a DB-backed permission, otherwise an "allow
        // once" mail decision would be re-prompted on every call.
        if matching.is_empty() {
            match session_status {
                Some(PermissionStatus::Granted) => return Ok(()),
                Some(PermissionStatus::Denied) => {
                    return Err(ExtensionError::permission_denied(
                        extension_id,
                        action.as_str(),
                        host,
                    ));
                }
                _ => {}
            }
            return Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "mail",
                action.as_str(),
                host,
            ));
        }

        // Single Denied blocks. (Granted/Ask are evaluated next.)
        if matching
            .iter()
            .any(|p| matches!(p.status, PermissionStatus::Denied))
        {
            return Err(ExtensionError::permission_denied(
                extension_id,
                action.as_str(),
                host,
            ));
        }

        if matching
            .iter()
            .any(|p| matches!(p.status, PermissionStatus::Granted))
        {
            return Ok(());
        }

        // Stored permissions are all Ask → consult session, then prompt.
        match session_status {
            Some(PermissionStatus::Granted) => return Ok(()),
            Some(PermissionStatus::Denied) => {
                return Err(ExtensionError::permission_denied(
                    extension_id,
                    action.as_str(),
                    host,
                ));
            }
            _ => {}
        }
        Err(ExtensionError::permission_prompt_required(
            extension_id,
            &extension.manifest.name,
            "mail",
            action.as_str(),
            host,
        ))
    }
}

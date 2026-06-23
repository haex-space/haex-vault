use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, IdentityAction, PermissionStatus, Principal, ResourceType,
};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Prüft Identitäten-Berechtigungen (`haex_identities`).
    ///
    /// Action-level wie `check_notifications_permission`; `target` ist immer
    /// "*". `IdentityAction::Read` und `IdentityAction::Write` sind DISTINCT
    /// capabilities — Write impliziert KEIN Read (siehe `IdentityAction`-Enum,
    /// Phase 3B). Read = DID/Name/Avatar/Notes lesen (NIEMALS `private_key`,
    /// erzwungen durch das `IdentityReadView`-DTO + `project_identity_read`).
    /// Write = ausschließlich neuen Kontakt anlegen (`validate_contact_insert`).
    ///
    /// Wird heute von keinem exponierten Command aufgerufen — Identitäten sind
    /// noch nicht an Principals exponiert. Existiert als Enforcement-Primitive,
    /// damit jede künftige Exposition zwingend hierdurch laufen muss.
    #[allow(dead_code)]
    pub async fn check_identities_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: IdentityAction,
    ) -> Result<(), ExtensionError> {
        let extension_id = principal.id();

        let (extension, permissions) =
            Self::load_extension_and_permissions(app_state, principal).await?;

        // Read und Write sind DISTINCT — exakter Action-Match, keine Hierarchie.
        let matching_status = identities_matching_status(&permissions, action);

        let session_granted = app_state.session_permissions.is_granted(
            extension_id,
            &Action::Identities(action),
            ResourceType::Identities,
            "*",
        );
        let session_denied = app_state.session_permissions.is_denied(
            extension_id,
            &Action::Identities(action),
            ResourceType::Identities,
            "*",
        );

        match resolve_identities_decision(matching_status, session_granted, session_denied) {
            IdentitiesDecision::Allow => Ok(()),
            IdentitiesDecision::Deny => Err(ExtensionError::permission_denied(
                extension_id,
                action.as_str(),
                "identities:*",
            )),
            IdentitiesDecision::Prompt => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "identities",
                action.as_str(),
                "*",
            )),
        }
    }
}

/// Das Ergebnis einer Identitäten-Permission-Entscheidung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentitiesDecision {
    Allow,
    Deny,
    Prompt,
}

/// Findet den `PermissionStatus` der DB-Permission, die exakt zur angefragten
/// [`IdentityAction`] passt (Read und Write sind DISTINCT — kein
/// `ReadWrite`-Implikation). `None`, wenn keine passende Row existiert.
///
/// Pure Helper, damit das sicherheitsrelevante Action-Matching unit-testbar
/// ist, ohne ein `State<AppState>` aufzubauen.
pub(crate) fn identities_matching_status(
    permissions: &[ExtensionPermission],
    action: IdentityAction,
) -> Option<PermissionStatus> {
    // Identities are wildcard-only (`target == "*"`); a sub-target row never
    // matches. Read and Write are DISTINCT (no escalation). Resolve deny-first
    // so a deny can never be hidden behind a grant if both are ever present.
    let statuses: Vec<PermissionStatus> = permissions
        .iter()
        .filter(|perm| {
            perm.resource_type == ResourceType::Identities
                && perm.target == "*"
                && matches!(&perm.action, Action::Identities(a) if *a == action)
        })
        .map(|perm| perm.status)
        .collect();

    if statuses.contains(&PermissionStatus::Denied) {
        Some(PermissionStatus::Denied)
    } else if statuses.contains(&PermissionStatus::Granted) {
        Some(PermissionStatus::Granted)
    } else if statuses.contains(&PermissionStatus::Ask) {
        Some(PermissionStatus::Ask)
    } else {
        None
    }
}

/// Löst die Identitäten-Permission-Entscheidung auf — identische Präzedenz wie
/// `check_notifications_permission`:
/// - DB-Permission gefunden: Granted → Allow, Denied → Deny, Ask → Prompt.
/// - Keine DB-Permission: Session-Grant → Allow, Session-Deny → Deny,
///   sonst → Prompt.
///
/// Pure Helper (kein `State`), damit die Entscheidungslogik unit-testbar ist.
pub(crate) fn resolve_identities_decision(
    matching_status: Option<PermissionStatus>,
    session_granted: bool,
    session_denied: bool,
) -> IdentitiesDecision {
    match matching_status {
        Some(PermissionStatus::Granted) => IdentitiesDecision::Allow,
        Some(PermissionStatus::Denied) => IdentitiesDecision::Deny,
        Some(PermissionStatus::Ask) => IdentitiesDecision::Prompt,
        None => {
            if session_granted {
                IdentitiesDecision::Allow
            } else if session_denied {
                IdentitiesDecision::Deny
            } else {
                IdentitiesDecision::Prompt
            }
        }
    }
}

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::session::SessionPermissionStore;
use crate::extension::permissions::types::{
    Action, BookmarksAction, ExtensionPermission, PermissionStatus, Principal, ResourceType,
};
use crate::AppState;
use tauri::State;

/// `true` iff `perm_action` (carried by a bookmarks permission row) grants
/// `required`. A `ReadWrite` permission implicitly covers a `Read` request.
fn action_allows(perm_action: &Action, required: BookmarksAction) -> bool {
    match perm_action {
        Action::Bookmarks(bookmarks_action) => match (bookmarks_action, required) {
            (a, b) if *a == b => true,
            (BookmarksAction::ReadWrite, BookmarksAction::Read) => true,
            _ => false,
        },
        _ => false,
    }
}

fn bookmarks_action_str(action: BookmarksAction) -> &'static str {
    match action {
        BookmarksAction::Read => "read",
        BookmarksAction::ReadWrite => "readWrite",
    }
}

impl PermissionManager {
    /// Prüft Bookmarks-Berechtigungen. Anders als `check_passwords_permission`
    /// gibt es keinen Tag-/Sammlungs-Scope: `bookmarks:read`/`readWrite` gilt
    /// für alle Sammlungen gleichermaßen — Sammlungstrennung ist Datenmodell-,
    /// kein Berechtigungskonzept. `target` ist daher immer `"*"`.
    pub async fn check_bookmarks_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: BookmarksAction,
    ) -> Result<(), ExtensionError> {
        let extension_id = principal.id();

        let (display_name, permissions) =
            Self::load_principal_display_name_and_permissions(app_state, principal).await?;

        let matching: Vec<&ExtensionPermission> = permissions
            .iter()
            .filter(|p| {
                p.resource_type == ResourceType::Bookmarks && action_allows(&p.action, action)
            })
            .collect();

        let action_str = bookmarks_action_str(action);

        if matching.is_empty() {
            if let Some(result) =
                bookmarks_session_status(&app_state.session_permissions, extension_id, action)
            {
                return result;
            }
            return Err(ExtensionError::permission_prompt_required(
                extension_id,
                &display_name,
                "bookmarks",
                action_str,
                "*",
            ));
        }

        // Ein einziges Denied blockiert alles (deny-first).
        if matching
            .iter()
            .any(|p| matches!(p.status, PermissionStatus::Denied))
        {
            return Err(ExtensionError::permission_denied(
                extension_id,
                action_str,
                "bookmarks:*",
            ));
        }

        let granted = matching
            .iter()
            .any(|p| matches!(p.status, PermissionStatus::Granted));

        if !granted {
            // Alle matchings sind Ask → erst Session-Store befragen, dann ggf. Prompt.
            if let Some(result) =
                bookmarks_session_status(&app_state.session_permissions, extension_id, action)
            {
                return result;
            }
            return Err(ExtensionError::permission_prompt_required(
                extension_id,
                &display_name,
                "bookmarks",
                action_str,
                "*",
            ));
        }

        Ok(())
    }
}

/// Resolves the bookmarks permission status from the **session** store,
/// mirroring the DB-row resolution above.
fn bookmarks_session_status(
    session: &SessionPermissionStore,
    extension_id: &str,
    action: BookmarksAction,
) -> Option<Result<(), ExtensionError>> {
    let session_permissions = session.get_permissions_for_extension(extension_id);

    let matching: Vec<&ExtensionPermission> = session_permissions
        .iter()
        .filter(|p| p.resource_type == ResourceType::Bookmarks && action_allows(&p.action, action))
        .collect();

    if matching.is_empty() {
        return None;
    }

    if matching
        .iter()
        .any(|p| matches!(p.status, PermissionStatus::Denied))
    {
        return Some(Err(ExtensionError::permission_denied(
            extension_id,
            bookmarks_action_str(action),
            "bookmarks:*",
        )));
    }

    if matching
        .iter()
        .any(|p| matches!(p.status, PermissionStatus::Granted))
    {
        return Some(Ok(()));
    }

    None
}

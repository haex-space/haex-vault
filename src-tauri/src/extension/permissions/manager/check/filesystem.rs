use crate::extension::error::ExtensionError;
#[cfg(desktop)]
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::manager::check::deny_first_precedence;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, PermissionConstraints, PermissionStatus, Principal, ResourceType,
};
use crate::AppState;
use std::path::Path;
use tauri::State;

impl PermissionManager {
    /// Prüft Dateisystem-Berechtigungen
    /// Returns PermissionPromptRequired if status is Ask or no permission exists
    /// Returns PermissionDenied if status is explicitly Denied
    pub async fn check_filesystem_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: Action,
        file_path: &Path,
    ) -> Result<(), ExtensionError> {
        let extension_id = principal.id();

        // Get extension for name lookup
        let (extension, permissions) =
            Self::load_extension_and_permissions(app_state, principal).await?;
        let file_path_str = file_path.to_string_lossy();

        // Resolve matching permissions deny-first, so an explicit `Denied` row
        // can never be hidden behind a `Granted` row regardless of insertion
        // order. Constraint-violating rows (e.g. file-extension allowlist) are
        // treated as `Denied` within the matching set, preserving the
        // pre-refactor "constraint failure = deny" semantics. See
        // `deny_first_precedence` for the precedence rules.
        let resolved = filesystem_matching_status(&permissions, &file_path_str, &action, file_path);

        match resolved {
            Some(PermissionStatus::Granted) => Ok(()),
            Some(PermissionStatus::Denied) => Err(ExtensionError::permission_denied(
                extension_id,
                &action.as_str(),
                &format!("filesystem path '{}'", file_path_str),
            )),
            Some(PermissionStatus::Ask) => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "fs",
                &action.as_str(),
                &file_path_str,
            )),
            // No matching permission in database - check session permissions
            None => {
                if app_state.session_permissions.is_granted(
                    extension_id,
                    &action,
                    ResourceType::Fs,
                    &file_path_str,
                ) {
                    return Ok(());
                }
                if app_state.session_permissions.is_denied(
                    extension_id,
                    &action,
                    ResourceType::Fs,
                    &file_path_str,
                ) {
                    return Err(ExtensionError::permission_denied(
                        extension_id,
                        &action.as_str(),
                        &format!("filesystem path '{}'", file_path_str),
                    ));
                }

                // No session permission either - prompt the user
                Err(ExtensionError::permission_prompt_required(
                    extension_id,
                    &extension.manifest.name,
                    "fs",
                    &action.as_str(),
                    &file_path_str,
                ))
            }
        }
    }

    /// Silent filesystem-read predicate — "may this extension read `file_path`
    /// *right now*, without prompting?".
    ///
    /// Unlike `check_filesystem_permission`, this never returns
    /// `PermissionPromptRequired`. Intended for fire-and-forget contexts
    /// (event broadcasting) where extensions that aren't definitely allowed
    /// are silently skipped.
    ///
    /// Loads the extension's DB permissions + session state, then delegates
    /// the actual decision to the pure `PermissionChecker::can_read_path_silently`
    /// (which is exhaustively unit-tested).
    ///
    /// Desktop-only: the sole caller is the `#[cfg(desktop)]` filesystem
    /// watcher's broadcast fan-out, so gating it here keeps the method off the
    /// Android build (where it would otherwise be dead code).
    #[cfg(desktop)]
    pub async fn is_fs_read_allowed_silently(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        file_path: &Path,
    ) -> bool {
        let principal = Principal::Extension(extension_id.to_string());
        let Ok(permissions) = Self::get_permissions(app_state, &principal).await else {
            return false;
        };
        let Some(extension) = app_state.extension_manager.get_extension(extension_id) else {
            return false;
        };

        let file_path_str = file_path.to_string_lossy();
        // Silent read predicate: the action is implicitly a filesystem Read.
        let read_action = Action::Filesystem(crate::extension::permissions::types::FsAction::Read);
        let session_granted = app_state.session_permissions.is_granted(
            extension_id,
            &read_action,
            ResourceType::Fs,
            &file_path_str,
        );
        let session_denied = app_state.session_permissions.is_denied(
            extension_id,
            &read_action,
            ResourceType::Fs,
            &file_path_str,
        );

        PermissionChecker::new(extension, permissions).can_read_path_silently(
            file_path,
            session_granted,
            session_denied,
        )
    }
}

/// Resolves the matching filesystem permission status for `(file_path, action)`
/// with **deny-first precedence**. Returns `None` when no FS row matches.
///
/// Constraint-violating rows (e.g. file-extension allowlist) are treated as
/// `Denied` within the matching set, preserving the pre-refactor semantics
/// where a single constraint-violating row caused a denial.
///
/// Pure helper (no `State<AppState>`) so the security-critical action+target
/// matching and deny-wins precedence are unit-testable, mirroring
/// `database_matching_status`.
pub(crate) fn filesystem_matching_status(
    permissions: &[ExtensionPermission],
    file_path_str: &str,
    action: &Action,
    file_path: &Path,
) -> Option<PermissionStatus> {
    let passes_constraints = |perm: &ExtensionPermission| -> bool {
        if let Some(PermissionConstraints::Filesystem(constraints)) = &perm.constraints {
            if let Some(allowed_ext) = &constraints.allowed_extensions {
                if let Some(ext) = file_path.extension() {
                    let ext_str = format!(".{}", ext.to_string_lossy());
                    if !allowed_ext.contains(&ext_str) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }
        true
    };

    deny_first_precedence(
        permissions
            .iter()
            .filter(|perm| {
                perm.resource_type == ResourceType::Fs
                    && perm.action == *action
                    && PermissionManager::matches_path_pattern(&perm.target, file_path_str)
            })
            .map(|perm| {
                if passes_constraints(perm) {
                    perm.status
                } else {
                    PermissionStatus::Denied
                }
            }),
    )
}

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, PermissionConstraints, PermissionStatus, Principal, ResourceType,
    ShellAction,
};
use crate::AppState;
use tauri::State;

impl PermissionManager {
    /// Prüft Shell-Berechtigungen
    /// Returns PermissionPromptRequired if status is Ask or no permission exists
    /// Returns PermissionDenied if status is explicitly Denied
    #[allow(dead_code)]
    pub async fn check_shell_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        command: &str,
        args: &[String],
    ) -> Result<(), ExtensionError> {
        let extension_id = principal.id();

        // Get extension for name lookup
        let (extension, permissions) =
            Self::load_extension_and_permissions(app_state, principal).await?;

        // Helper to check if command matches target pattern
        let matches_command = |target: &str| -> bool { target == command || target == "*" };

        // Helper to check constraints
        let passes_constraints = |perm: &ExtensionPermission| -> bool {
            if let Some(PermissionConstraints::Shell(constraints)) = &perm.constraints {
                if let Some(allowed_subcommands) = &constraints.allowed_subcommands {
                    if !args.is_empty()
                        && !allowed_subcommands.contains(&args[0])
                        && !allowed_subcommands.contains(&"*".to_string())
                    {
                        return false;
                    }
                }

                if let Some(forbidden) = &constraints.forbidden_args {
                    if args.iter().any(|arg| forbidden.contains(arg)) {
                        return false;
                    }
                }

                if let Some(allowed_flags) = &constraints.allowed_flags {
                    let user_flags: Vec<_> =
                        args.iter().filter(|arg| arg.starts_with('-')).collect();

                    for flag in user_flags {
                        if !allowed_flags.contains(flag)
                            && !allowed_flags.contains(&"*".to_string())
                        {
                            return false;
                        }
                    }
                }
            }
            true
        };

        // Find matching permission for this command
        let matching_permission = permissions.iter().find(|perm| {
            perm.resource_type == ResourceType::Shell && matches_command(&perm.target)
        });

        match matching_permission {
            Some(perm) => {
                if !passes_constraints(perm) {
                    return Err(ExtensionError::permission_denied(
                        extension_id,
                        "execute",
                        &format!(
                            "shell command '{}' with args {:?} (constraint violation)",
                            command, args
                        ),
                    ));
                }
                match perm.status {
                    PermissionStatus::Granted => Ok(()),
                    PermissionStatus::Denied => Err(ExtensionError::permission_denied(
                        extension_id,
                        "execute",
                        &format!("shell command '{}' with args {:?}", command, args),
                    )),
                    PermissionStatus::Ask => Err(ExtensionError::permission_prompt_required(
                        extension_id,
                        &extension.manifest.name,
                        "shell",
                        "execute",
                        command,
                    )),
                }
            }
            // No matching permission in database - check session permissions
            None => {
                if app_state.session_permissions.is_granted(
                    extension_id,
                    &Action::Shell(ShellAction::Execute),
                    ResourceType::Shell,
                    command,
                ) {
                    return Ok(());
                }
                if app_state.session_permissions.is_denied(
                    extension_id,
                    &Action::Shell(ShellAction::Execute),
                    ResourceType::Shell,
                    command,
                ) {
                    return Err(ExtensionError::permission_denied(
                        extension_id,
                        "execute",
                        &format!("shell command '{}' with args {:?}", command, args),
                    ));
                }

                // No session permission either - prompt the user
                Err(ExtensionError::permission_prompt_required(
                    extension_id,
                    &extension.manifest.name,
                    "shell",
                    "execute",
                    command,
                ))
            }
        }
    }
}

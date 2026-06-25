use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::check::deny_first_precedence;
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

        match shell_matching_status(&permissions, command, args) {
            Some(PermissionStatus::Granted) => Ok(()),
            Some(PermissionStatus::Denied) => {
                let has_constraint_violation =
                    shell_matching_has_constraint_violation(&permissions, command, args);
                Err(ExtensionError::permission_denied(
                    extension_id,
                    "execute",
                    &format_shell_denied_target(command, args, has_constraint_violation),
                ))
            }
            Some(PermissionStatus::Ask) => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "shell",
                "execute",
                command,
            )),
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
                        &format_shell_denied_target(command, args, false),
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

/// Resolves the matching shell permission status for `(command, args)` with
/// **deny-first precedence**. Returns `None` when no shell row matches the
/// command target.
///
/// Constraint-violating rows (e.g. `forbidden_args`, `allowed_subcommands`,
/// `allowed_flags` rejected the args) resolve to `Denied` within the matching
/// set, preserving the pre-refactor semantics where a single constraint-
/// violating row caused a denial.
///
/// Pure helper (no `State<AppState>`) so the security-critical command
/// matching and deny-wins precedence are unit-testable, mirroring
/// `filesystem_matching_status` / `database_matching_status` /
/// `web_matching_status`.
pub(crate) fn shell_matching_status(
    permissions: &[ExtensionPermission],
    command: &str,
    args: &[String],
) -> Option<PermissionStatus> {
    deny_first_precedence(matching_shell_rows(permissions, command).map(|perm| {
        if shell_constraints_pass(perm, args) {
            perm.status
        } else {
            PermissionStatus::Denied
        }
    }))
}

/// Returns `true` iff ANY row in the shell matching set fails its constraints
/// (`forbidden_args`, `allowed_subcommands`, `allowed_flags`). Callers combine
/// this with the resolved status from [`shell_matching_status`] to decide
/// whether a denial diagnostic should be tagged with the pre-refactor
/// `(constraint violation)` suffix.
pub(crate) fn shell_matching_has_constraint_violation(
    permissions: &[ExtensionPermission],
    command: &str,
    args: &[String],
) -> bool {
    matching_shell_rows(permissions, command).any(|perm| !shell_constraints_pass(perm, args))
}

/// Formats the `target` field of a shell `permission_denied` error, preserving
/// the pre-refactor diagnostic discriminator `" (constraint violation)"` when
/// the denial was caused by a constraint-violating row rather than an explicit
/// `Denied` permission.
pub(crate) fn format_shell_denied_target(
    command: &str,
    args: &[String],
    has_constraint_violation: bool,
) -> String {
    if has_constraint_violation {
        format!(
            "shell command '{}' with args {:?} (constraint violation)",
            command, args
        )
    } else {
        format!("shell command '{}' with args {:?}", command, args)
    }
}

/// Iterator over shell permission rows whose `(resource_type, target)` matches
/// the given `command`. Constraint evaluation is intentionally NOT included
/// here — callers (status resolver, constraint flag) consume the same matching
/// set with different downstream rules.
fn matching_shell_rows<'a>(
    permissions: &'a [ExtensionPermission],
    command: &'a str,
) -> impl Iterator<Item = &'a ExtensionPermission> {
    permissions.iter().filter(move |perm| {
        perm.resource_type == ResourceType::Shell && (perm.target == command || perm.target == "*")
    })
}

/// `true` iff `perm`'s shell constraints (`allowed_subcommands`,
/// `forbidden_args`, `allowed_flags`) are satisfied by `args`. Rows without
/// shell constraints always pass.
fn shell_constraints_pass(perm: &ExtensionPermission, args: &[String]) -> bool {
    let Some(PermissionConstraints::Shell(constraints)) = &perm.constraints else {
        return true;
    };

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
        let user_flags: Vec<_> = args.iter().filter(|arg| arg.starts_with('-')).collect();

        for flag in user_flags {
            if !allowed_flags.contains(flag) && !allowed_flags.contains(&"*".to_string()) {
                return false;
            }
        }
    }

    true
}

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::session::SessionPermissionStore;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, PasswordsAction, PasswordsScope, PermissionStatus, Principal,
    ResourceType,
};
use crate::AppState;
use serde_json::Value as JsonValue;
use tauri::State;

/// `true` iff `perm_action` (carried by a passwords permission row) grants
/// `required`. A `ReadWrite` permission implicitly covers a `Read` request.
/// Shared between the DB-row and session-row matching paths.
fn action_allows(perm_action: &Action, required: &PasswordsAction) -> bool {
    match perm_action {
        Action::Passwords(passwords_action) => match (passwords_action, required) {
            (a, b) if a == b => true,
            (PasswordsAction::ReadWrite, PasswordsAction::Read) => true,
            _ => false,
        },
        _ => false,
    }
}

fn passwords_action_str(action: &PasswordsAction) -> &'static str {
    match action {
        PasswordsAction::Read => "read",
        PasswordsAction::ReadWrite => "readWrite",
    }
}

impl PermissionManager {
    /// Prüft Passwörter-Berechtigungen und liefert den erlaubten Tag-Scope zurück.
    ///
    /// Der Scope wird über `ExtensionPermission.target` gesteuert:
    ///   - target = "*"        → Zugriff auf alle Einträge (PasswordsScope::All)
    ///   - target = "calendar" → nur Einträge mit Tag "calendar"
    ///   - mehrere Grants      → Union der Tags (sofern kein "*" dabei)
    ///
    /// Es zählen nur `Granted`-Permissions. Ist keine passende `Granted`
    /// vorhanden, wird abhängig vom Zustand bestehender Einträge entweder
    /// `Denied` oder `PromptRequired` zurückgegeben — analog zu den anderen
    /// `check_*_permission`-Funktionen. Fehlt eine DB-Permission komplett (oder
    /// sind alle vorhandenen DB-Rows nur `Ask`), wird zusätzlich der
    /// Session-Store befragt (siehe `passwords_session_scope`), analog zu
    /// `web`/`rw_resource`/`spaces`.
    pub async fn check_passwords_permission(
        app_state: &State<'_, AppState>,
        principal: &Principal,
        action: PasswordsAction,
    ) -> Result<PasswordsScope, ExtensionError> {
        let extension_id = principal.id();

        let (extension, permissions) =
            Self::load_extension_and_permissions(app_state, principal).await?;

        let matching: Vec<&ExtensionPermission> = permissions
            .iter()
            .filter(|p| {
                p.resource_type == ResourceType::Passwords && action_allows(&p.action, &action)
            })
            .collect();

        let action_str = passwords_action_str(&action);

        if matching.is_empty() {
            if let Some(result) =
                passwords_session_scope(&app_state.session_permissions, extension_id, action)
            {
                return result;
            }
            return Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "passwords",
                action_str,
                "*",
            ));
        }

        // Prüfe auf ein Denied — ein einziges Denied blockiert alles.
        if matching
            .iter()
            .any(|p| matches!(p.status, PermissionStatus::Denied))
        {
            return Err(ExtensionError::permission_denied(
                extension_id,
                action_str,
                "passwords:*",
            ));
        }

        let granted: Vec<&&ExtensionPermission> = matching
            .iter()
            .filter(|p| matches!(p.status, PermissionStatus::Granted))
            .collect();

        if granted.is_empty() {
            // Alle matchings sind Ask → erst Session-Store befragen, dann ggf.
            // Prompt mit den tatsächlich vom Manifest deklarierten Tags (statt
            // eines pauschalen "*"), damit der Nutzer sieht, wonach die
            // Erweiterung wirklich fragt.
            if let Some(result) =
                passwords_session_scope(&app_state.session_permissions, extension_id, action)
            {
                return result;
            }

            let ask_targets: Vec<String> = matching.iter().map(|p| p.target.clone()).collect();
            let ask_target = if ask_targets.iter().any(|t| t == "*") {
                "*".to_string()
            } else {
                ask_targets.join(",")
            };

            return Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "passwords",
                action_str,
                &ask_target,
            ));
        }

        // Wildcard "*" schlägt alle Tags — Vollzugriff.
        if granted.iter().any(|p| p.target == "*") {
            return Ok(PasswordsScope::All);
        }

        // Non-wildcard Tag-Scope: pro Row das Default-Label-Marker aus den rohen
        // Passwords-Constraints (`{"default":true}`) lesen. `get_permissions`
        // trägt diese rohen Constraints bereits in `raw_constraints` (der
        // typisierte untagged-Enum kann sie nicht repräsentieren), also keine
        // zweite SQL-Abfrage nötig.
        let rows: Vec<PasswordsGrantRow> = granted
            .iter()
            .map(|p| PasswordsGrantRow {
                target: p.target.clone(),
                is_default: parse_passwords_default_marker(p.raw_constraints.as_ref()),
            })
            .collect();

        // Default-Label nur beim Schreiben (Erstellen) relevant.
        let write_granted = matches!(action, PasswordsAction::ReadWrite);
        resolve_passwords_tags_scope(rows, write_granted, extension_id)
    }
}

/// Liest das Passwords-Default-Marker-Flag aus den rohen Constraints einer
/// Permission-Row. Passwords markieren ihre Default-Label-Row per free-form
/// `{"default": true}` (das der typisierte [`PermissionConstraints`]-Enum nicht
/// repräsentieren kann). Fehlende/abweichende Constraints ⇒ `false`.
pub(crate) fn parse_passwords_default_marker(raw: Option<&JsonValue>) -> bool {
    #[derive(serde::Deserialize)]
    struct DefaultMarker {
        #[serde(default)]
        default: bool,
    }
    raw.and_then(|v| serde_json::from_value::<DefaultMarker>(v.clone()).ok())
        .map(|m| m.default)
        .unwrap_or(false)
}

/// Eine gewährte (non-wildcard) Passwords-Permission-Row, reduziert auf die
/// für die Scope-Auflösung relevanten Felder.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PasswordsGrantRow {
    /// Das Tag/Label, auf das diese Row Zugriff gewährt.
    pub target: String,
    /// Ob diese Row explizit als Default-Label markiert ist (`{"default":true}`).
    pub is_default: bool,
}

/// Löst die gewährten (non-wildcard) Passwords-Tag-Rows zu einem
/// [`PasswordsScope::Tags`] inkl. aufgelöstem Default-Label auf.
///
/// Aufrufer-Vertrag: wird NUR aufgerufen, wenn KEINE Wildcard-`*`-Row gewährt
/// ist (die wird vorher direkt zu [`PasswordsScope::All`] aufgelöst) und `rows`
/// nicht leer ist.
///
/// Regeln (sicherheitsrelevant):
/// - Genau EIN erlaubtes Label → dieses ist implizit der Default (keine
///   explizite Markierung nötig).
/// - MEHRERE Labels + `write_granted` → es muss GENAU EINE Row als Default
///   markiert sein. Null oder >1 markierte Rows ⇒ der Grant ist ungültig ⇒
///   Reject ([`ExtensionError::SecurityViolation`]). Das markierte Default ist
///   per Konstruktion eines der erlaubten Labels (es IST eine der Rows).
/// - MEHRERE Labels, NUR Read (kein Write) → kein Default nötig (`None`); eine
///   etwaige Markierung wird ignoriert, da Defaults nur beim Erstellen zählen.
pub(crate) fn resolve_passwords_tags_scope(
    rows: Vec<PasswordsGrantRow>,
    write_granted: bool,
    extension_id: &str,
) -> Result<PasswordsScope, ExtensionError> {
    let tags: Vec<String> = rows.iter().map(|r| r.target.clone()).collect();

    // Genau ein erlaubtes Label → implizit der Default.
    if tags.len() == 1 {
        return Ok(PasswordsScope::Tags {
            default: Some(tags[0].clone()),
            tags,
        });
    }

    // Mehrere Labels.
    if !write_granted {
        // Read-only: kein Default nötig.
        return Ok(PasswordsScope::Tags {
            tags,
            default: None,
        });
    }

    // Mehrere Labels + Write: genau eine Row muss als Default markiert sein.
    let marked: Vec<&PasswordsGrantRow> = rows.iter().filter(|r| r.is_default).collect();
    match marked.as_slice() {
        [single] => Ok(PasswordsScope::Tags {
            default: Some(single.target.clone()),
            tags,
        }),
        [] => Err(ExtensionError::SecurityViolation {
            reason: format!(
                "Passwords write grant for extension '{extension_id}' allows multiple labels \
                 ({tags:?}) but none is marked as the default (constraints {{\"default\":true}}). \
                 Exactly one default label is required."
            ),
        }),
        _ => Err(ExtensionError::SecurityViolation {
            reason: format!(
                "Passwords write grant for extension '{extension_id}' marks multiple default \
                 labels ({:?}). Exactly one default label is required.",
                marked.iter().map(|r| &r.target).collect::<Vec<_>>()
            ),
        }),
    }
}

/// Resolves the passwords permission status from the **session** store,
/// mirroring the DB-row resolution above (wildcard shortcut, tag-scope +
/// default-label rules via [`resolve_passwords_tags_scope`]).
///
/// Returns:
/// - `None` — no session entry at all (or all session rows are `Ask`, which
///   session grants never actually are, but is handled defensively the same
///   way) → caller falls through to `PermissionPromptRequired`.
/// - `Some(Err(permission_denied))` — an explicit session deny blocks access.
/// - `Some(Ok(scope))` — a usable session grant (wildcard or tag-scope).
pub(crate) fn passwords_session_scope(
    session: &SessionPermissionStore,
    extension_id: &str,
    action: PasswordsAction,
) -> Option<Result<PasswordsScope, ExtensionError>> {
    let session_permissions = session.get_permissions_for_extension(extension_id);

    let matching: Vec<&ExtensionPermission> = session_permissions
        .iter()
        .filter(|p| p.resource_type == ResourceType::Passwords && action_allows(&p.action, &action))
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
            passwords_action_str(&action),
            "passwords:*",
        )));
    }

    let granted: Vec<&&ExtensionPermission> = matching
        .iter()
        .filter(|p| matches!(p.status, PermissionStatus::Granted))
        .collect();

    if granted.is_empty() {
        return None;
    }

    if granted.iter().any(|p| p.target == "*") {
        return Some(Ok(PasswordsScope::All));
    }

    let rows: Vec<PasswordsGrantRow> = granted
        .iter()
        .map(|p| PasswordsGrantRow {
            target: p.target.clone(),
            is_default: parse_passwords_default_marker(p.raw_constraints.as_ref()),
        })
        .collect();

    let write_granted = matches!(action, PasswordsAction::ReadWrite);
    Some(resolve_passwords_tags_scope(
        rows,
        write_granted,
        extension_id,
    ))
}

/// Cleans and validates a user-submitted passwords tag grant (from the
/// permission-prompt dialog) before it's persisted, shared by the DB
/// (`resolve_permission_prompt`) and session (`grant_session_permission`)
/// write paths.
///
/// - Trims, drops empty entries, and dedupes `tags` (order-preserving).
/// - `"*"` anywhere in the input collapses the whole grant to the wildcard
///   (`vec!["*"]`) — mirrors the DB/session read-side wildcard shortcut.
/// - Enforces the same "exactly one default label" invariant as
///   [`resolve_passwords_tags_scope`] up front, at the point of writing a
///   grant, rather than only discovering an invalid state later when a
///   `check_passwords_permission` call trips [`ExtensionError::SecurityViolation`].
///
/// Returns `(effective_tags, is_wildcard)`.
pub(crate) fn normalize_passwords_grant_tags(
    tags: &[String],
    default_tag: Option<&str>,
    action: PasswordsAction,
    status: PermissionStatus,
    extension_id: &str,
) -> Result<(Vec<String>, bool), ExtensionError> {
    let mut seen = std::collections::HashSet::new();
    let cleaned: Vec<String> = tags
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .filter(|t| seen.insert(t.clone()))
        .collect();

    let is_wildcard = cleaned.iter().any(|t| t == "*");
    let effective = if is_wildcard {
        vec!["*".to_string()]
    } else {
        cleaned
    };

    if effective.is_empty() {
        return Err(ExtensionError::ValidationError {
            reason: "At least one tag (or '*' for all tags) is required".to_string(),
        });
    }

    if status == PermissionStatus::Granted
        && !is_wildcard
        && effective.len() > 1
        && matches!(action, PasswordsAction::ReadWrite)
    {
        let has_valid_default = default_tag.is_some_and(|d| effective.iter().any(|t| t == d));
        if !has_valid_default {
            return Err(ExtensionError::ValidationError {
                reason: format!(
                    "extension '{extension_id}': defaultTag must be set to one of the granted \
                     tags ({effective:?}) when granting write access to multiple tags"
                ),
            });
        }
    }

    Ok((effective, is_wildcard))
}

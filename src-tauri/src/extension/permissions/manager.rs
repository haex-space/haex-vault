use crate::database::core::{select_with_crdt, with_connection};
use crate::database::error::DatabaseError;
use crate::database::generated::HaexExtensionPermissions;
use crate::extension::core::types::ExtensionSource;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::checker::PermissionChecker;
use crate::extension::permissions::types::{
    Action, ExtensionPermission, PermissionConstraints, PermissionStatus, ResourceType,
};
use crate::table_names::TABLE_EXTENSION_PERMISSIONS;
use crate::AppState;
use rusqlite::params;
use serde_json::Value as JsonValue;
use std::path::Path;
use tauri::State;

pub struct PermissionManager;

impl PermissionManager {
    /// Speichert alle Permissions einer Extension
    pub async fn save_permissions(
        app_state: &State<'_, AppState>,
        permissions: &[ExtensionPermission],
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            let sql = format!(
                "INSERT INTO {TABLE_EXTENSION_PERMISSIONS} (id, extension_id, resource_type, action, target, constraints, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
            );

            for perm in permissions {
                // 1. Konvertiere App-Struct zu DB-Struct
                let db_perm: HaexExtensionPermissions = perm.into();

                // 2. Erstelle typsichere Parameter
                let params = params![
                    db_perm.id,
                    db_perm.extension_id,
                    db_perm.resource_type,
                    db_perm.action,
                    db_perm.target,
                    db_perm.constraints,
                    db_perm.status,
                ];

                // 3. Führe mit dem typsicheren Executor aus
                // HINWEIS: Du musst eine `execute_internal_typed` Funktion erstellen!
                SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params)?;
            }

            tx.commit().map_err(DatabaseError::from)?;
            Ok(())
        })
        .map_err(ExtensionError::from)
    }

    /// Aktualisiert eine Permission
    pub async fn update_permission(
        app_state: &State<'_, AppState>,
        permission: &ExtensionPermission,
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            let db_perm: HaexExtensionPermissions = permission.into();
            
            let sql = format!(
                "UPDATE {TABLE_EXTENSION_PERMISSIONS} SET resource_type = ?, action = ?, target = ?, constraints = ?, status = ? WHERE id = ?"
            );

            let params = params![
                db_perm.resource_type,
                db_perm.action,
                db_perm.target,
                db_perm.constraints,
                db_perm.status,
                db_perm.id,
            ];

            SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params)?;
            tx.commit().map_err(DatabaseError::from)
        })
        .map_err(ExtensionError::from)
    }

    /// Ändert den Status einer Permission
    pub async fn update_permission_status(
        app_state: &State<'_, AppState>,
        permission_id: &str,
        new_status: PermissionStatus,
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            let sql = format!("UPDATE {TABLE_EXTENSION_PERMISSIONS} SET status = ? WHERE id = ?");
            let params = params![new_status.as_str(), permission_id];
            SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params)?;
            tx.commit().map_err(DatabaseError::from)
        })
        .map_err(ExtensionError::from)
    }

    /// Löscht alle Permissions einer Extension
    pub async fn delete_permission(
        app_state: &State<'_, AppState>,
        permission_id: &str,
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            // Echtes DELETE - wird vom CrdtTransformer zu UPDATE umgewandelt
            let sql = format!("DELETE FROM {TABLE_EXTENSION_PERMISSIONS} WHERE id = ?");
            SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params![permission_id])?;
            tx.commit().map_err(DatabaseError::from)
        })
        .map_err(ExtensionError::from)
    }

    /// Löscht alle Permissions einer Extension (Soft-Delete)
    pub async fn delete_permissions(
        app_state: &State<'_, AppState>,
        extension_id: &str,
    ) -> Result<(), ExtensionError> {
        with_connection(&app_state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = app_state
                .hlc
                .lock()
                .map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

            let sql = format!("DELETE FROM {TABLE_EXTENSION_PERMISSIONS} WHERE extension_id = ?");
            SqlExecutor::execute_internal_typed(&tx, &hlc_service, &sql, params![extension_id])?;
            tx.commit().map_err(DatabaseError::from)
        })
        .map_err(ExtensionError::from)
    }

    /// Löscht alle Permissions einer Extension innerhalb einer bestehenden Transaktion
    pub fn delete_permissions_in_transaction(
        tx: &rusqlite::Transaction,
        hlc_service: &crate::crdt::hlc::HlcService,
        extension_id: &str,
    ) -> Result<(), DatabaseError> {
        let sql = format!("DELETE FROM {TABLE_EXTENSION_PERMISSIONS} WHERE extension_id = ?");
        SqlExecutor::execute_internal_typed(tx, hlc_service, &sql, params![extension_id])?;
        Ok(())
    }
    /// Lädt alle Permissions einer Extension
    /// Uses select_with_crdt to automatically filter out tombstoned (soft-deleted) entries
    pub async fn get_permissions(
        app_state: &State<'_, AppState>,
        extension_id: &str,
    ) -> Result<Vec<ExtensionPermission>, ExtensionError> {
        let sql = format!(
            "SELECT id, extension_id, resource_type, action, target, constraints, status, haex_timestamp FROM {TABLE_EXTENSION_PERMISSIONS} WHERE extension_id = ?"
        );
        let params = vec![JsonValue::String(extension_id.to_string())];

        let results = select_with_crdt(sql, params, &app_state.db)?;

        let permissions = results
            .into_iter()
            .map(|row| {
                let resource_type = row[2]
                    .as_str()
                    .and_then(|s| ResourceType::from_str(s).ok())
                    .unwrap_or(ResourceType::Db);
                let action = row[3]
                    .as_str()
                    .and_then(|s| Action::from_str(&resource_type, s).ok())
                    .unwrap_or(Action::Database(crate::extension::permissions::types::DbAction::Read));
                let status = row[6]
                    .as_str()
                    .and_then(|s| PermissionStatus::from_str(s).ok())
                    .unwrap_or(PermissionStatus::Denied);
                let constraints: Option<PermissionConstraints> = row[5]
                    .as_str()
                    .and_then(|s| serde_json::from_str(s).ok());

                ExtensionPermission {
                    id: row[0].as_str().unwrap_or_default().to_string(),
                    extension_id: row[1].as_str().unwrap_or_default().to_string(),
                    resource_type,
                    action,
                    target: row[4].as_str().unwrap_or_default().to_string(),
                    constraints,
                    status,
                    haex_timestamp: row[7].as_str().map(String::from),
                }
            })
            .collect();

        Ok(permissions)
    }

    /// Prüft Datenbankberechtigungen
    /// Returns PermissionPromptRequired if status is Ask or no permission exists
    /// Returns PermissionDenied if status is explicitly Denied
    pub async fn check_database_permission(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        action: Action,
        table_name: &str,
    ) -> Result<(), ExtensionError> {
        // Extract DbAction from Action enum
        let db_action = match action {
            Action::Database(db_action) => db_action,
            _ => {
                return Err(ExtensionError::ValidationError {
                    reason: "Expected database action".to_string(),
                });
            }
        };

        // Get the extension
        let extension = app_state
            .extension_manager
            .get_extension(extension_id)
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: format!("Extension with ID {extension_id} not found"),
            })?
            .clone();

        // Load permissions
        let permissions = Self::get_permissions(app_state, extension_id).await?;

        // Create checker and validate
        let checker = PermissionChecker::new(extension.clone(), permissions.clone());

        // First check if auto-allowed (extension's own tables)
        if checker.is_auto_allowed_table(table_name) {
            return Ok(());
        }

        // Find matching permission for this table and action
        let matching_permission = permissions.iter().find(|perm| {
            perm.resource_type == ResourceType::Db
                && checker.matches_table_pattern(&perm.target, table_name)
                && checker.action_allows_db_action(&perm.action, db_action)
        });

        match matching_permission {
            Some(perm) => match perm.status {
                PermissionStatus::Granted => Ok(()),
                PermissionStatus::Denied => Err(ExtensionError::permission_denied(
                    extension_id,
                    &format!("{db_action:?}"),
                    &format!("database table '{table_name}'"),
                )),
                PermissionStatus::Ask => Err(ExtensionError::permission_prompt_required(
                    extension_id,
                    &extension.manifest.name,
                    "db",
                    &format!("{db_action:?}"),
                    table_name,
                )),
            },
            // No matching permission - prompt the user
            None => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "db",
                &format!("{db_action:?}"),
                table_name,
            )),
        }
    }

    /// Prüft Web-Berechtigungen für Requests
    /// Method/operation is not checked - only protocol, domain, port, and path
    /// Returns PermissionPromptRequired if status is Ask or no permission exists
    /// Returns PermissionDenied if status is explicitly Denied
    pub async fn check_web_permission(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        url: &str,
    ) -> Result<(), ExtensionError> {
        // Get extension for name lookup
        let extension = app_state
            .extension_manager
            .get_extension(extension_id)
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: format!("Extension not found: {}", extension_id),
            })?
            .clone();

        // Load permissions - for dev extensions, get from manifest; for production, from database
        let permissions: Vec<ExtensionPermission> = match &extension.source {
            ExtensionSource::Development { .. } => {
                // Dev extension - get web permissions from manifest
                extension
                    .manifest
                    .permissions
                    .to_internal_permissions(extension_id)
                    .into_iter()
                    .filter(|p| p.resource_type == ResourceType::Web)
                    .map(|mut p| {
                        // Dev extensions have all permissions granted by default
                        p.status = PermissionStatus::Granted;
                        p
                    })
                    .collect()
            }
            ExtensionSource::Production { .. } => {
                // Production extension - load from database using select_with_crdt
                // to automatically filter out tombstoned (soft-deleted) entries
                let sql = format!(
                    "SELECT id, extension_id, resource_type, action, target, constraints, status, haex_timestamp FROM {TABLE_EXTENSION_PERMISSIONS} WHERE extension_id = ? AND resource_type = 'web'"
                );
                let params = vec![JsonValue::String(extension_id.to_string())];

                let results = select_with_crdt(sql, params, &app_state.db)?;

                results
                    .into_iter()
                    .map(|row| {
                        let resource_type = row[2]
                            .as_str()
                            .and_then(|s| ResourceType::from_str(s).ok())
                            .unwrap_or(ResourceType::Web);
                        let action = row[3]
                            .as_str()
                            .and_then(|s| Action::from_str(&resource_type, s).ok())
                            .unwrap_or(Action::Web(crate::extension::permissions::types::WebAction::Get));
                        let status = row[6]
                            .as_str()
                            .and_then(|s| PermissionStatus::from_str(s).ok())
                            .unwrap_or(PermissionStatus::Denied);
                        let constraints: Option<PermissionConstraints> = row[5]
                            .as_str()
                            .and_then(|s| serde_json::from_str(s).ok());

                        ExtensionPermission {
                            id: row[0].as_str().unwrap_or_default().to_string(),
                            extension_id: row[1].as_str().unwrap_or_default().to_string(),
                            resource_type,
                            action,
                            target: row[4].as_str().unwrap_or_default().to_string(),
                            constraints,
                            status,
                            haex_timestamp: row[7].as_str().map(String::from),
                        }
                    })
                    .collect()
            }
        };

        let url_parsed = url::Url::parse(url).map_err(|e| ExtensionError::ValidationError {
            reason: format!("Invalid URL: {}", e),
        })?;

        let domain = url_parsed
            .host_str()
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: "URL does not contain a valid host".to_string(),
            })?;

        // Find matching permission for this URL
        let matching_permission = permissions.iter().find(|perm| {
            let url_matches = if perm.target == "*" {
                true
            } else if perm.target.contains("://") {
                Self::matches_url_pattern(&perm.target, url)
            } else {
                perm.target == domain || domain.ends_with(&format!(".{}", perm.target))
            };
            url_matches
        });

        match matching_permission {
            Some(perm) => match perm.status {
                PermissionStatus::Granted => Ok(()),
                PermissionStatus::Denied => Err(ExtensionError::permission_denied(
                    extension_id,
                    "web request",
                    url,
                )),
                PermissionStatus::Ask => Err(ExtensionError::permission_prompt_required(
                    extension_id,
                    &extension.manifest.name,
                    "web",
                    "request",
                    url,
                )),
            },
            // No matching permission - prompt the user
            None => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "web",
                "request",
                url,
            )),
        }
    }

    /// Prüft Dateisystem-Berechtigungen
    /// Returns PermissionPromptRequired if status is Ask or no permission exists
    /// Returns PermissionDenied if status is explicitly Denied
    pub async fn check_filesystem_permission(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        action: Action,
        file_path: &Path,
    ) -> Result<(), ExtensionError> {
        // Get extension for name lookup
        let extension = app_state
            .extension_manager
            .get_extension(extension_id)
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: format!("Extension not found: {}", extension_id),
            })?
            .clone();

        let permissions = Self::get_permissions(app_state, extension_id).await?;
        let file_path_str = file_path.to_string_lossy();

        // Find matching permission for this path and action
        let matching_permission = permissions.iter().find(|perm| {
            perm.resource_type == ResourceType::Fs
                && perm.action == action
                && Self::matches_path_pattern(&perm.target, &file_path_str)
        });

        // Check constraints if we have a matching permission
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

        match matching_permission {
            Some(perm) => {
                if !passes_constraints(perm) {
                    return Err(ExtensionError::permission_denied(
                        extension_id,
                        &format!("{:?}", action),
                        &format!("filesystem path '{}' (constraint violation)", file_path_str),
                    ));
                }
                match perm.status {
                    PermissionStatus::Granted => Ok(()),
                    PermissionStatus::Denied => Err(ExtensionError::permission_denied(
                        extension_id,
                        &format!("{:?}", action),
                        &format!("filesystem path '{}'", file_path_str),
                    )),
                    PermissionStatus::Ask => Err(ExtensionError::permission_prompt_required(
                        extension_id,
                        &extension.manifest.name,
                        "fs",
                        &format!("{:?}", action),
                        &file_path_str,
                    )),
                }
            }
            // No matching permission - prompt the user
            None => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "fs",
                &format!("{:?}", action),
                &file_path_str,
            )),
        }
    }

    /// Prüft Shell-Berechtigungen
    /// Returns PermissionPromptRequired if status is Ask or no permission exists
    /// Returns PermissionDenied if status is explicitly Denied
    pub async fn check_shell_permission(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        command: &str,
        args: &[String],
    ) -> Result<(), ExtensionError> {
        // Get extension for name lookup
        let extension = app_state
            .extension_manager
            .get_extension(extension_id)
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: format!("Extension not found: {}", extension_id),
            })?
            .clone();

        let permissions = Self::get_permissions(app_state, extension_id).await?;

        // Helper to check if command matches target pattern
        let matches_command = |target: &str| -> bool {
            target == command || target == "*"
        };

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
                        &format!("shell command '{}' with args {:?} (constraint violation)", command, args),
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
            // No matching permission - prompt the user
            None => Err(ExtensionError::permission_prompt_required(
                extension_id,
                &extension.manifest.name,
                "shell",
                "execute",
                command,
            )),
        }
    }

    // Helper-Methoden - müssen DatabaseError statt ExtensionError zurückgeben
    pub fn parse_resource_type(s: &str) -> Result<ResourceType, DatabaseError> {
        match s {
            "fs" => Ok(ResourceType::Fs),
            "web" => Ok(ResourceType::Web),
            "db" => Ok(ResourceType::Db),
            "shell" => Ok(ResourceType::Shell),
            _ => Err(DatabaseError::SerializationError {
                reason: format!("Unknown resource type: {s}"),
            }),
        }
    }

    fn matches_path_pattern(pattern: &str, path: &str) -> bool {
        if let Some(prefix) = pattern.strip_suffix("/*") {
            return path.starts_with(prefix);
        }

        if pattern.starts_with("*.") {
            let suffix = &pattern[1..];
            return path.ends_with(suffix);
        }

        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                return path.starts_with(parts[0]) && path.ends_with(parts[1]);
            }
        }

        pattern == path || pattern == "*"
    }

    /// Matches a URL against a URL pattern
    /// Supports:
    /// - Path wildcards: "https://domain.com/*"
    /// - Subdomain wildcards: "https://*.domain.com/*"
    fn matches_url_pattern(pattern: &str, url: &str) -> bool {
        // Parse the actual URL
        let Ok(url_parsed) = url::Url::parse(url) else {
            return false;
        };

        // Check if pattern contains subdomain wildcard
        let has_subdomain_wildcard = pattern.contains("://*.") || pattern.starts_with("*.");

        if has_subdomain_wildcard {
            // Extract components for wildcard matching
            // Pattern: "https://*.example.com/*"

            // Get protocol from pattern
            let protocol_end = pattern.find("://").unwrap_or(0);
            let pattern_protocol = if protocol_end > 0 {
                &pattern[..protocol_end]
            } else {
                ""
            };

            // Protocol must match if specified
            if !pattern_protocol.is_empty() && pattern_protocol != url_parsed.scheme() {
                return false;
            }

            // Extract the domain pattern (after *.  )
            let domain_start = if pattern.contains("://*.") {
                pattern.find("://*.").unwrap() + 5 // length of "://.*"
            } else if pattern.starts_with("*.") {
                2 // length of "*."
            } else {
                return false;
            };

            // Find where the domain pattern ends (at / or end of string)
            let domain_pattern_end = pattern[domain_start..]
                .find('/')
                .map(|i| domain_start + i)
                .unwrap_or(pattern.len());
            let domain_pattern = &pattern[domain_start..domain_pattern_end];

            // Check if the URL's host ends with the domain pattern
            let Some(url_host) = url_parsed.host_str() else {
                return false;
            };

            // Match: *.example.com should match subdomain.example.com but not example.com
            // Also match: exact domain if no subdomain wildcard prefix
            if !url_host.ends_with(domain_pattern) && url_host != domain_pattern {
                return false;
            }

            // For subdomain wildcard, ensure there's actually a subdomain
            if pattern.contains("*.") && url_host == domain_pattern {
                return false; // *.example.com should NOT match example.com
            }

            // Check path wildcard if present
            if pattern.contains("/*") {
                // Any path is allowed
                return true;
            }

            // Check exact path if no wildcard
            let pattern_path_start = domain_pattern_end;
            if pattern_path_start < pattern.len() {
                let pattern_path = &pattern[pattern_path_start..];
                return url_parsed.path() == pattern_path;
            }

            return true;
        }

        // No subdomain wildcard - parse as full URL
        let Ok(pattern_url) = url::Url::parse(pattern) else {
            return false;
        };

        // Protocol must match
        if pattern_url.scheme() != url_parsed.scheme() {
            return false;
        }

        // Host must match
        if pattern_url.host_str() != url_parsed.host_str() {
            return false;
        }

        // Port must match (if specified)
        if pattern_url.port() != url_parsed.port() {
            return false;
        }

        // Path matching with wildcard support
        if pattern.contains("/*") {
            // Extract the base path before the wildcard
            if let Some(wildcard_pos) = pattern.find("/*") {
                let pattern_before_wildcard = &pattern[..wildcard_pos];
                return url.starts_with(pattern_before_wildcard);
            }
        }

        // Exact path match (no wildcard)
        pattern_url.path() == url_parsed.path()
    }
}

// Convenience-Funktionen für die verschiedenen Subsysteme
/* impl PermissionManager {
    // Convenience-Methoden
    pub async fn can_read_file(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        file_path: &Path,
    ) -> Result<(), ExtensionError> {
        Self::check_filesystem_permission(app_state, extension_id, Action::Read, file_path).await
    }

    pub async fn can_write_file(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        file_path: &Path,
    ) -> Result<(), ExtensionError> {
        Self::check_filesystem_permission(app_state, extension_id, Action::Write, file_path).await
    }

    pub async fn can_read_table(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        table_name: &str,
    ) -> Result<(), ExtensionError> {
        Self::check_database_permission(app_state, extension_id, Action::Read, table_name).await
    }

    pub async fn can_write_table(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        table_name: &str,
    ) -> Result<(), ExtensionError> {
        Self::check_database_permission(app_state, extension_id, Action::Write, table_name).await
    }

    pub async fn can_http_get(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        url: &str,
    ) -> Result<(), ExtensionError> {
        Self::check_http_permission(app_state, extension_id, "GET", url).await
    }

    pub async fn can_http_post(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        url: &str,
    ) -> Result<(), ExtensionError> {
        Self::check_http_permission(app_state, extension_id, "POST", url).await
    }

    pub async fn can_execute_command(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        command: &str,
        args: &[String],
    ) -> Result<(), ExtensionError> {
        Self::check_shell_permission(app_state, extension_id, command, args).await
    }

    pub async fn grant_permission(
        app_state: &State<'_, AppState>,
        permission_id: &str,
    ) -> Result<(), ExtensionError> {
        Self::update_permission_status(app_state, permission_id, PermissionStatus::Granted).await
    }

    pub async fn deny_permission(
        app_state: &State<'_, AppState>,
        permission_id: &str,
    ) -> Result<(), ExtensionError> {
        Self::update_permission_status(app_state, permission_id, PermissionStatus::Denied).await
    }

    pub async fn ask_permission(
        app_state: &State<'_, AppState>,
        permission_id: &str,
    ) -> Result<(), ExtensionError> {
        Self::update_permission_status(app_state, permission_id, PermissionStatus::Ask).await
    }

    pub async fn get_ask_permissions(
        app_state: &State<'_, AppState>,
        extension_id: &str,
    ) -> Result<Vec<ExtensionPermission>, ExtensionError> {
        let all_permissions = Self::get_permissions(app_state, extension_id).await?;
        Ok(all_permissions
            .into_iter()
            .filter(|perm| perm.status == PermissionStatus::Ask)
            .collect())
    }
} */

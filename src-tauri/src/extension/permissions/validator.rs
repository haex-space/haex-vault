// src-tauri/src/extension/permissions/validator.rs

use crate::database::core::{
    extract_table_names_from_sql, extract_table_names_from_statement, parse_single_statement,
};
use crate::database::error::DatabaseError;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::Action;
use crate::AppState;
use sqlparser::ast::Statement;
use tauri::State;

pub struct SqlPermissionValidator;

#[allow(dead_code)]
impl SqlPermissionValidator {
    /// Validiert ein SQL-Statement gegen die Permissions einer Extension
    pub async fn validate_sql(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        sql: &str,
    ) -> Result<(), ExtensionError> {
        let statement = parse_single_statement(sql).map_err(|e| DatabaseError::ParseError {
            reason: e.to_string(),
            sql: sql.to_string(),
        })?;

        match &statement {
            Statement::Query(_) => {
                Self::validate_read_statement(app_state, extension_id, sql).await
            }
            Statement::Insert(_) | Statement::Update { .. } | Statement::Delete(_) => {
                Self::validate_write_statement(app_state, extension_id, &statement).await
            }
            // Schema modification statements (CREATE TABLE, ALTER TABLE, DROP) are NOT allowed
            // through regular SQL execution. They can only be executed during:
            // - Extension installation (migrations)
            // - Synchronization of migrations from other devices
            Statement::CreateTable(_) | Statement::AlterTable { .. } | Statement::Drop { .. } => {
                Err(ExtensionError::ValidationError {
                    reason: "Schema modifications (CREATE TABLE, ALTER TABLE, DROP) are only allowed during extension installation and synchronization".to_string(),
                })
            }
            _ => Err(ExtensionError::ValidationError {
                reason: format!("Statement type not allowed: {sql}"),
            }),
        }
    }

    /// Validiert READ-Operationen (SELECT)
    async fn validate_read_statement(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        sql: &str,
    ) -> Result<(), ExtensionError> {
        let tables = extract_table_names_from_sql(sql)?;

        for table_name in tables {
            PermissionManager::check_database_permission(
                app_state,
                extension_id,
                Action::Database(super::types::DbAction::Read),
                &table_name,
            )
            .await?;
        }

        Ok(())
    }

    /// Validiert WRITE-Operationen (INSERT, UPDATE, DELETE)
    async fn validate_write_statement(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        statement: &Statement,
    ) -> Result<(), ExtensionError> {
        let table_names = Self::extract_table_names(statement);

        for table_name in table_names {
            PermissionManager::check_database_permission(
                app_state,
                extension_id,
                Action::Database(super::types::DbAction::ReadWrite),
                &table_name,
            )
            .await?;
        }

        Ok(())
    }

    /// Validiert CREATE TABLE
    /// Extensions können nur Tabellen mit ihrem eigenen Prefix erstellen:
    /// Format: {public_key}__{extension_name}__{table_name}
    async fn validate_create_statement(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        statement: &Statement,
    ) -> Result<(), ExtensionError> {
        if let Statement::CreateTable(create_table) = statement {
            let table_name = create_table.name.to_string();
            let clean_table_name = table_name.trim_matches('"').trim_matches('`');

            // Get extension to retrieve public_key and name
            let extension = app_state
                .extension_manager
                .get_extension(extension_id)
                .ok_or_else(|| ExtensionError::ValidationError {
                    reason: format!("Extension with ID {} not found", extension_id),
                })?;

            // Extensions can ONLY create tables with their own prefix
            let expected_prefix = crate::extension::utils::get_extension_table_prefix(
                &extension.manifest.public_key,
                &extension.manifest.name,
            );

            if !clean_table_name.starts_with(&expected_prefix) {
                return Err(ExtensionError::ValidationError {
                    reason: format!(
                        "Extension can only create tables with prefix '{}'. Got: '{}'",
                        expected_prefix, clean_table_name
                    ),
                });
            }

            // Also check if extension has CREATE permission
            PermissionManager::check_database_permission(
                app_state,
                extension_id,
                Action::Database(super::types::DbAction::Create),
                clean_table_name,
            )
            .await?;
        }

        Ok(())
    }

    /// Validiert Schema-Änderungen (ALTER, DROP)
    /// Extensions können nur ihre eigenen Tabellen ändern/löschen
    async fn validate_schema_statement(
        app_state: &State<'_, AppState>,
        extension_id: &str,
        statement: &Statement,
    ) -> Result<(), ExtensionError> {
        let table_names = Self::extract_table_names(statement);

        // Get extension to retrieve public_key and name
        let extension = app_state
            .extension_manager
            .get_extension(extension_id)
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: format!("Extension with ID {} not found", extension_id),
            })?;

        let expected_prefix = crate::extension::utils::get_extension_table_prefix(
            &extension.manifest.public_key,
            &extension.manifest.name,
        );

        for table_name in table_names {
            let clean_table_name = table_name.trim_matches('"').trim_matches('`');

            // Extensions can ONLY alter/drop their own tables
            if !clean_table_name.starts_with(&expected_prefix) {
                return Err(ExtensionError::ValidationError {
                    reason: format!(
                        "Extension can only alter/drop tables with prefix '{}'. Got: '{}'",
                        expected_prefix, clean_table_name
                    ),
                });
            }

            // Also check if extension has ALTER/DROP permission
            PermissionManager::check_database_permission(
                app_state,
                extension_id,
                Action::Database(super::types::DbAction::AlterDrop),
                clean_table_name,
            )
            .await?;
        }

        Ok(())
    }

    /// Delegates to core::extract_table_names_from_statement for full recursive extraction
    fn extract_table_names(statement: &Statement) -> Vec<String> {
        extract_table_names_from_statement(statement)
    }
}

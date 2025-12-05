// src-tauri/src/crdt/transformer.rs

use crate::crdt::insert_transformer::InsertTransformer;
use crate::crdt::trigger::{COLUMN_HLCS_COLUMN, HLC_TIMESTAMP_COLUMN, TOMBSTONE_COLUMN};
use crate::database::error::DatabaseError;
use sqlparser::ast::{
    Assignment, AssignmentTarget, BinaryOperator, ColumnDef, DataType, Expr, FromTable, Ident,
    ObjectName, ObjectNamePart, Query, Select, SetExpr, Statement, TableFactor, TableObject,
    Value,
};
use std::borrow::Cow;
use uhlc::Timestamp;

/// Konfiguration für CRDT-Spalten
#[derive(Clone)]
struct CrdtColumns {
    hlc_timestamp: &'static str,
    column_hlcs: &'static str,
    tombstone: &'static str,
}

impl CrdtColumns {
    const DEFAULT: Self = Self {
        hlc_timestamp: HLC_TIMESTAMP_COLUMN,
        column_hlcs: COLUMN_HLCS_COLUMN,
        tombstone: TOMBSTONE_COLUMN,
    };

    /// Erstellt eine HLC-Zuweisung für UPDATE/DELETE
    fn create_hlc_assignment(&self, timestamp: &Timestamp) -> Assignment {
        Assignment {
            target: AssignmentTarget::ColumnName(ObjectName(vec![ObjectNamePart::Identifier(
                Ident::new(self.hlc_timestamp),
            )])),
            value: Expr::Value(Value::SingleQuotedString(timestamp.to_string()).into()),
        }
    }

    /// Erstellt eine WHERE-Bedingung für haex_tombstone = 0
    fn create_tombstone_filter(&self) -> Expr {
        Expr::BinaryOp {
            left: Box::new(Expr::Identifier(Ident::new(self.tombstone))),
            op: BinaryOperator::Eq,
            right: Box::new(Expr::Value(Value::Number("0".to_string(), false).into())),
        }
    }

    /// Prüft ob ein Ausdruck bereits eine haex_tombstone Bedingung enthält
    fn has_tombstone_condition(&self, expr: &Expr) -> bool {
        match expr {
            // Direkte Bedingung: haex_tombstone = X
            Expr::BinaryOp { left, op, .. } => {
                if matches!(op, BinaryOperator::Eq) {
                    if let Expr::Identifier(ident) = left.as_ref() {
                        if ident.value == self.tombstone {
                            return true;
                        }
                    }
                }
                // Rekursiv in verschachtelten BinaryOps suchen (AND, OR)
                if matches!(op, BinaryOperator::And | BinaryOperator::Or) {
                    if let Expr::BinaryOp { left, right, .. } = expr {
                        return self.has_tombstone_condition(left)
                            || self.has_tombstone_condition(right);
                    }
                }
                false
            }
            // In anderen Ausdrücken könnte auch haex_tombstone vorkommen
            _ => false,
        }
    }

    /// Fügt haex_tombstone = 0 zu einer WHERE-Klausel hinzu
    /// Nur wenn noch keine haex_tombstone Bedingung vorhanden ist
    fn add_tombstone_filter_to_where(&self, existing_where: Option<Expr>) -> Option<Expr> {
        // Prüfe ob bereits eine haex_tombstone Bedingung existiert
        if let Some(ref where_expr) = existing_where {
            if self.has_tombstone_condition(where_expr) {
                // Bedingung bereits vorhanden - nicht hinzufügen
                return existing_where;
            }
        }

        let tombstone_filter = self.create_tombstone_filter();

        match existing_where {
            Some(existing) => Some(Expr::BinaryOp {
                left: Box::new(existing),
                op: BinaryOperator::And,
                right: Box::new(tombstone_filter),
            }),
            None => Some(tombstone_filter),
        }
    }

    /// Fügt CRDT-Spalten zu einer Tabellendefinition hinzu
    /// Überschreibt vorhandene Spalten mit den gleichen Namen, um korrekte Datentypen zu garantieren
    fn add_to_table_definition(&self, columns: &mut Vec<ColumnDef>) {
        // Remove existing CRDT columns if present
        columns.retain(|c| {
            c.name.value != self.hlc_timestamp
                && c.name.value != self.column_hlcs
                && c.name.value != self.tombstone
        });

        // Add all CRDT columns with correct types
        columns.push(ColumnDef {
            name: Ident::new(self.hlc_timestamp),
            data_type: DataType::String(None),
            options: vec![],
        });

        columns.push(ColumnDef {
            name: Ident::new(self.column_hlcs),
            data_type: DataType::String(None),
            options: vec![],
        });

        columns.push(ColumnDef {
            name: Ident::new(self.tombstone),
            data_type: DataType::Int(None),
            options: vec![],
        });
    }
}

pub struct CrdtTransformer {
    columns: CrdtColumns,
}

impl CrdtTransformer {
    pub fn new() -> Self {
        Self {
            columns: CrdtColumns::DEFAULT,
        }
    }

    /// Prüft, ob eine Tabelle CRDT-Synchronisation unterstützen soll
    fn is_crdt_sync_table(&self, name: &ObjectName) -> bool {
        let table_name = self.normalize_table_name(name);

        // Exclude all haex_crdt_* tables (internal CRDT metadata)
        // This includes: haex_crdt_changes, haex_crdt_configs, haex_crdt_snapshots, haex_crdt_sync_status
        if table_name.starts_with("haex_crdt_") {
            return false;
        }

        true
    }

    /// Normalisiert Tabellennamen (entfernt Anführungszeichen und Schema-Präfix wie "main.")
    fn normalize_table_name(&self, name: &ObjectName) -> Cow<str> {
        // Get the last part of the ObjectName (the actual table name without schema)
        // This handles cases like "main.tablename" where we only want "tablename"
        let table_name = name
            .0
            .last()
            .map(|part| match part {
                ObjectNamePart::Identifier(ident) => ident.value.clone(),
                ObjectNamePart::Function(func) => func.name.to_string(),
            })
            .unwrap_or_else(|| name.to_string());

        let name_str = table_name.to_lowercase();
        Cow::Owned(name_str.trim_matches('`').trim_matches('"').to_string())
    }

    /// Prüft ob ein TableWithJoins eine CRDT-Tabelle referenziert
    fn is_crdt_table_with_joins(&self, table: &sqlparser::ast::TableWithJoins) -> bool {
        if let TableFactor::Table { name, .. } = &table.relation {
            self.is_crdt_sync_table(name)
        } else {
            false
        }
    }

    /// Transformiert ein SELECT Statement (fügt WHERE haex_tombstone = 0 hinzu)
    fn transform_select(&self, select: &mut Select) {
        // Prüfe ob mindestens eine CRDT-Tabelle in FROM vorhanden ist
        let has_crdt_table = select.from.iter().any(|t| self.is_crdt_table_with_joins(t));

        if has_crdt_table {
            // Füge WHERE haex_tombstone = 0 hinzu (falls noch nicht vorhanden)
            select.selection = self
                .columns
                .add_tombstone_filter_to_where(select.selection.take());
        }
    }

    /// Transformiert eine Query rekursiv (SELECT, UNION, etc.)
    pub fn transform_query(&self, query: &mut Query) {
        self.transform_set_expr(&mut query.body);
    }

    /// Transformiert einen SetExpr rekursiv
    fn transform_set_expr(&self, set_expr: &mut SetExpr) {
        match set_expr {
            SetExpr::Select(select) => {
                self.transform_select(select);
            }
            SetExpr::Query(query) => {
                self.transform_query(query);
            }
            SetExpr::SetOperation { left, right, .. } => {
                self.transform_set_expr(left);
                self.transform_set_expr(right);
            }
            _ => {
                // Values, Insert, Update, Delete, Merge, Table - keine Transformation nötig
            }
        }
    }

    // =================================================================
    // ÖFFENTLICHE API-METHODEN
    // =================================================================

    /// Transformiert ein SQL Statement für CRDT-Synchronisation
    ///
    /// Gibt `Some(table_name)` zurück wenn das Schema modifiziert wurde (CREATE TABLE, ALTER TABLE)
    /// Gibt `None` zurück für Daten-Operationen (INSERT, UPDATE, DELETE)
    pub fn transform_execute_statement(
        &self,
        stmt: &mut Statement,
        hlc_timestamp: &Timestamp,
    ) -> Result<Option<String>, DatabaseError> {
        match stmt {
            Statement::Query(query) => {
                // Transform SELECT queries to add WHERE haex_tombstone = 0
                self.transform_query(query);
                Ok(None)
            }
            Statement::CreateTable(create_table) => {
                if self.is_crdt_sync_table(&create_table.name) {
                    self.columns
                        .add_to_table_definition(&mut create_table.columns);
                    Ok(Some(
                        self.normalize_table_name(&create_table.name).into_owned(),
                    ))
                } else {
                    Ok(None)
                }
            }
            Statement::Insert(insert_stmt) => {
                if let TableObject::TableName(name) = &insert_stmt.table {
                    if self.is_crdt_sync_table(name) {
                        // Hard Delete: Kein Schema-Lookup mehr nötig (kein ON CONFLICT)
                        let insert_transformer = InsertTransformer::new();
                        insert_transformer.transform_insert(insert_stmt, hlc_timestamp)?;
                    }
                }
                Ok(None)
            }
            Statement::Update {
                table,
                assignments,
                selection,
                ..
            } => {
                if let TableFactor::Table { name, .. } = &table.relation {
                    if self.is_crdt_sync_table(name) {
                        // Add HLC timestamp assignment
                        assignments.push(self.columns.create_hlc_assignment(hlc_timestamp));

                        // Add WHERE haex_tombstone = 0 to only update non-deleted rows
                        // (unless WHERE haex_tombstone = 1 is already present)
                        *selection = self
                            .columns
                            .add_tombstone_filter_to_where(selection.take());
                    }
                }
                Ok(None)
            }
            Statement::Delete(del_stmt) => {
                // Soft Delete: Transform DELETE into UPDATE with haex_tombstone = 1
                // Extract the table from FromTable enum
                let from_tables = match &del_stmt.from {
                    FromTable::WithFromKeyword(tables) => tables,
                    FromTable::WithoutKeyword(tables) => tables,
                };

                if let Some(from_table) = from_tables.first() {
                    if let TableFactor::Table { name, .. } = &from_table.relation {
                        if self.is_crdt_sync_table(name) {
                            // Create tombstone assignment
                            let tombstone_assignment = Assignment {
                                target: AssignmentTarget::ColumnName(ObjectName(vec![
                                    ObjectNamePart::Identifier(Ident::new(self.columns.tombstone)),
                                ])),
                                value: Expr::Value(Value::Number("1".to_string(), false).into()),
                            };

                            // Create HLC assignment
                            let hlc_assignment = self.columns.create_hlc_assignment(hlc_timestamp);

                            // Transform DELETE into UPDATE
                            *stmt = Statement::Update {
                                table: from_table.clone(),
                                assignments: vec![tombstone_assignment, hlc_assignment],
                                from: None,
                                selection: del_stmt.selection.clone(),
                                returning: None,
                                or: None,
                                limit: None,
                            };
                        }
                    }
                }
                Ok(None)
            }
            Statement::AlterTable { name, .. } => {
                if self.is_crdt_sync_table(name) {
                    Ok(Some(self.normalize_table_name(name).into_owned()))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }
}

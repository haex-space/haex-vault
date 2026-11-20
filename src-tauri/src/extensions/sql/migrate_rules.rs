//! The rules of extension migrations (contracts/sql-policy.md §Migrationen, FR-033): what a
//! Drizzle migration may contain before any of it runs. A migration that breaks one rule is
//! refused as a whole.

use std::collections::HashSet;
use std::ops::ControlFlow;

use haex_crdt::sqlparser::ast::{
    AlterTableOperation, ColumnOption, Expr, ObjectName, ObjectNamePart, ObjectType, Query,
    SetExpr, Statement, TableConstraint, TableFactor, TableObject, Visit, Visitor,
};

use super::ast_check::{check_words, table_name};
use super::parse::{parse_one, violation};
use super::{classify, function_allowed, TableClass, TABLE_FUNCTIONS};
use crate::extensions::error::BridgeError;
use crate::extensions::ids::TablePrefix;

/// The separator Drizzle writes between the statements of a migration.
pub const BREAKPOINT: &str = "--> statement-breakpoint";

/// One step of a checked migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// `CREATE`, `ALTER` or `DROP`, run as written; SQLite rewrites its schema tables meanwhile.
    Schema(String),
    /// `INSERT`, `UPDATE`, `DELETE` or a query of the own tables, run as written.
    Data(String),
    /// The row copy of a table rebuild (`INSERT INTO __new_… SELECT … FROM …`): run with the CRDT
    /// columns copied unchanged (haex-crdt `copy_rows_verbatim`).
    CopyVerbatim(String),
}

/// A table of the extension or the temporary table of a Drizzle rebuild (`__new_<own table>`).
pub fn own_or_rebuild(name: &str, own: &TablePrefix) -> bool {
    let name = name.to_ascii_lowercase();
    let base = name.strip_prefix("__new_").unwrap_or(&name);
    matches!(classify(base, own), TableClass::Own(_))
}

fn is_no_sync(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with("_no_sync")
}

fn single_name(name: &ObjectName) -> Result<String, BridgeError> {
    match name.0.as_slice() {
        [ObjectNamePart::Identifier(ident)] => Ok(ident.value.to_ascii_lowercase()),
        _ => Err(violation("only plain names are allowed in a migration")),
    }
}

fn own_table(name: &ObjectName, own: &TablePrefix) -> Result<String, BridgeError> {
    let name = table_name(name)?;
    if own_or_rebuild(&name, own) {
        Ok(name)
    } else {
        Err(violation("a migration may only touch the own tables"))
    }
}

/// Every table a DML statement or query reads or writes must be the extension's own; functions
/// from the allowlist only.
struct OwnTables<'a> {
    own: &'a TablePrefix,
    ctes: HashSet<String>,
    relations: Vec<String>,
}

impl Visitor for OwnTables<'_> {
    type Break = BridgeError;

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<BridgeError> {
        if !matches!(
            *query.body,
            SetExpr::Select(_)
                | SetExpr::Query(_)
                | SetExpr::SetOperation { .. }
                | SetExpr::Values(_)
        ) {
            return ControlFlow::Break(violation("statement kind not allowed"));
        }
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.ctes.insert(cte.alias.name.value.to_ascii_lowercase());
            }
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<BridgeError> {
        match factor {
            TableFactor::Table {
                name,
                args: Some(_),
                ..
            } => match table_name(name) {
                Ok(f) if TABLE_FUNCTIONS.contains(&f.as_str()) => {
                    self.ctes.insert(f);
                    ControlFlow::Continue(())
                }
                _ => ControlFlow::Break(violation("function not allowed")),
            },
            TableFactor::Table { .. }
            | TableFactor::Derived { .. }
            | TableFactor::NestedJoin { .. } => ControlFlow::Continue(()),
            _ => ControlFlow::Break(violation("table form not allowed")),
        }
    }

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<BridgeError> {
        match table_name(relation) {
            Ok(name) => {
                self.relations.push(name);
                ControlFlow::Continue(())
            }
            Err(e) => ControlFlow::Break(e),
        }
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<BridgeError> {
        if let Expr::Function(function) = expr {
            let allowed = matches!(
                function.name.0.as_slice(),
                [ObjectNamePart::Identifier(ident)] if function_allowed(&ident.value)
            );
            if !allowed {
                return ControlFlow::Break(violation("function not allowed"));
            }
        }
        ControlFlow::Continue(())
    }
}

fn only_own_tables(statement: &Statement, own: &TablePrefix) -> Result<(), BridgeError> {
    let mut visitor = OwnTables {
        own,
        ctes: HashSet::new(),
        relations: Vec::new(),
    };
    if let ControlFlow::Break(e) = statement.visit(&mut visitor) {
        return Err(e);
    }
    let ctes = visitor.ctes;
    if visitor
        .relations
        .iter()
        .filter(|r| !ctes.contains(*r))
        .all(|r| own_or_rebuild(r, visitor.own))
    {
        Ok(())
    } else {
        Err(violation("a migration may only touch the own tables"))
    }
}

/// `REFERENCES` only to own tables (a foreign key to haex-vault's tables would tie their deletes).
fn check_references(statement: &Statement, own: &TablePrefix) -> Result<(), BridgeError> {
    let Statement::CreateTable(create) = statement else {
        return Ok(());
    };
    let columns = create
        .columns
        .iter()
        .flat_map(|c| &c.options)
        .filter_map(|o| match &o.option {
            ColumnOption::ForeignKey(fk) => Some(&fk.foreign_table),
            _ => None,
        });
    let constraints = create.constraints.iter().filter_map(|c| match c {
        TableConstraint::ForeignKey(fk) => Some(&fk.foreign_table),
        _ => None,
    });
    for referenced in columns.chain(constraints) {
        own_table(referenced, own)?;
    }
    Ok(())
}

/// The index names of a migration carry the own prefix, so they never collide with haex-vault's.
fn own_index(name: &ObjectName, own: &TablePrefix) -> Result<(), BridgeError> {
    let name = single_name(name)?;
    if name.starts_with(&own.to_string()) {
        Ok(())
    } else {
        Err(violation(
            "index names must start with the own table prefix",
        ))
    }
}

fn check_statement(
    statement: &Statement,
    text: &str,
    own: &TablePrefix,
) -> Result<Option<Step>, BridgeError> {
    let sql = text.to_owned();
    match statement {
        // Only switches the schema mode, which every migration runs in anyway.
        Statement::Pragma { name, .. } if single_name(name).is_ok_and(|n| n == "foreign_keys") => {
            Ok(None)
        }
        Statement::CreateTable(create) => {
            own_table(&create.name, own)?;
            if create.temporary
                || create.query.is_some()
                || create.clone.is_some()
                || create.like.is_some()
            {
                return Err(violation(
                    "CREATE TABLE may not be TEMP, AS SELECT, LIKE or CLONE",
                ));
            }
            check_references(statement, own)?;
            Ok(Some(Step::Schema(sql)))
        }
        Statement::CreateIndex(create) => {
            own_table(&create.table_name, own)?;
            match &create.name {
                Some(name) => own_index(name, own)?,
                None => return Err(violation("an index needs a name")),
            }
            Ok(Some(Step::Schema(sql)))
        }
        Statement::AlterTable(alter) => {
            let table = own_table(&alter.name, own)?;
            for operation in &alter.operations {
                match operation {
                    AlterTableOperation::AddColumn { .. }
                    | AlterTableOperation::DropColumn { .. }
                    | AlterTableOperation::RenameColumn { .. } => {}
                    AlterTableOperation::RenameTable { table_name: to } => {
                        let to = match to {
                            haex_crdt::sqlparser::ast::RenameTableNameKind::As(n)
                            | haex_crdt::sqlparser::ast::RenameTableNameKind::To(n) => n,
                        };
                        let to = own_table(to, own)?;
                        if is_no_sync(&to) != is_no_sync(&table) {
                            return Err(violation("a rename may not cross the _no_sync boundary"));
                        }
                    }
                    _ => return Err(violation("ALTER TABLE form not allowed")),
                }
            }
            Ok(Some(Step::Schema(sql)))
        }
        Statement::Drop {
            object_type, names, ..
        } => {
            for name in names {
                match object_type {
                    ObjectType::Table => drop(own_table(name, own)?),
                    ObjectType::Index => own_index(name, own)?,
                    _ => return Err(violation("DROP form not allowed")),
                }
            }
            Ok(Some(Step::Schema(sql)))
        }
        Statement::Insert(insert) => {
            only_own_tables(statement, own)?;
            let rebuild = match &insert.table {
                TableObject::TableName(name) => table_name(name)?.starts_with("__new_"),
                _ => false,
            };
            Ok(Some(if rebuild && insert.source.is_some() {
                Step::CopyVerbatim(sql)
            } else {
                Step::Data(sql)
            }))
        }
        Statement::Update(_) | Statement::Delete(_) | Statement::Query(_) => {
            only_own_tables(statement, own)?;
            Ok(Some(Step::Data(sql)))
        }
        _ => Err(violation("statement not allowed in a migration")),
    }
}

/// `PRAGMA foreign_keys = ON|OFF|0|1` (Drizzle writes it around a rebuild); sqlparser does not
/// read the `ON`/`OFF` form.
fn is_foreign_keys_switch(part: &str) -> bool {
    let compact: String = part
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let compact = compact.trim_end_matches(';');
    matches!(
        compact,
        "pragmaforeign_keys=on"
            | "pragmaforeign_keys=off"
            | "pragmaforeign_keys=0"
            | "pragmaforeign_keys=1"
    )
}

/// Checks a whole migration and returns its steps, or the first rule it breaks.
pub fn plan(migration: &str, own: &TablePrefix) -> Result<Vec<Step>, BridgeError> {
    let mut steps = Vec::new();
    for part in migration.split(BREAKPOINT) {
        let part = part.trim();
        if part.is_empty() || is_foreign_keys_switch(part) {
            continue;
        }
        check_words(part)?;
        let statement = parse_one(part)?;
        if let Some(step) = check_statement(&statement, part, own)? {
            steps.push(step);
        }
    }
    Ok(steps)
}

#[cfg(test)]
#[path = "migrate_rules_tests.rs"]
mod tests;

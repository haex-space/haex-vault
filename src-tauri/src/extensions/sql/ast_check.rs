//! The pre-check of extension SQL at run time (contracts/sql-policy.md §Vorprüfung): what a
//! statement may be and which tables it reads and writes.
//!
//! Ported from haex-space/haex-vault-legacy `8dce379d94e18fcd42c3b73686a06f984ca3f574`,
//! `src-tauri/src/database/core/extract.rs` (`extract_table_names_from_statement`) and
//! `src-tauri/src/extension/permissions/validator.rs` (`validate_sql`). The gaps of research R6
//! point 1 are closed: tables come from sqlparser's visitor over the whole tree (`WITH`,
//! `EXISTS`, subqueries in every clause, `JOIN … ON`, `CASE`, function arguments, `RETURNING`,
//! `ON CONFLICT`), the statement's own `WITH` names are subtracted and may not shadow a real
//! table, only `main` or no qualifier, ASCII identifiers, no sync column by name, functions only
//! from the allowlist, and a core table is refused instead of asked for.

use std::collections::HashSet;
use std::ops::ControlFlow;

use haex_crdt::sqlparser::ast::{
    Expr, ObjectName, ObjectNamePart, Query, SetExpr, Statement, TableFactor, TableObject, Visit,
    Visitor,
};

use super::parse::{violation, words};
use super::{classify, function_allowed, TableClass, TABLE_FUNCTIONS};
use crate::extensions::error::BridgeError;
use crate::extensions::ids::{ExtensionTable, TablePrefix};

/// The extension tables a statement needs, by what it does with them. A table it writes is not
/// listed again under `reads`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequiredAccess {
    pub reads: Vec<ExtensionTable>,
    pub writes: Vec<ExtensionTable>,
}

impl RequiredAccess {
    /// Every table, read or written.
    pub fn tables(&self) -> impl Iterator<Item = &ExtensionTable> {
        self.reads.iter().chain(&self.writes)
    }
}

/// `main.t` and `t` name the same table; any other qualifier or more parts are refused.
pub(crate) fn table_name(name: &ObjectName) -> Result<String, BridgeError> {
    let parts: Vec<&str> = name
        .0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident) => Ok(ident.value.as_str()),
            _ => Err(violation("table name not allowed")),
        })
        .collect::<Result<_, _>>()?;
    match parts.as_slice() {
        [table] => Ok(table.to_ascii_lowercase()),
        [schema, table] if schema.eq_ignore_ascii_case("main") => Ok(table.to_ascii_lowercase()),
        _ => Err(violation("only tables of the main database are allowed")),
    }
}

/// Collects what the visitor sees; the first rule a node breaks stops the walk.
#[derive(Default)]
struct Collector {
    relations: Vec<String>,
    ctes: HashSet<String>,
    table_functions: HashSet<String>,
}

impl Visitor for Collector {
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
            TableFactor::Table { name, args, .. } => {
                if args.is_some() {
                    match table_name(name) {
                        Ok(function) if TABLE_FUNCTIONS.contains(&function.as_str()) => {
                            self.table_functions.insert(function);
                        }
                        Ok(_) => return ControlFlow::Break(violation("function not allowed")),
                        Err(e) => return ControlFlow::Break(e),
                    }
                }
                ControlFlow::Continue(())
            }
            TableFactor::Derived { .. } | TableFactor::NestedJoin { .. } => {
                ControlFlow::Continue(())
            }
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
            let allowed = match function.name.0.as_slice() {
                [ObjectNamePart::Identifier(ident)] => function_allowed(&ident.value),
                _ => false,
            };
            if !allowed {
                return ControlFlow::Break(violation("function not allowed"));
            }
        }
        ControlFlow::Continue(())
    }
}

/// The tables a write statement writes, as written.
pub(crate) fn write_targets(statement: &Statement) -> Result<Vec<String>, BridgeError> {
    Ok(match statement {
        Statement::Insert(insert) => match &insert.table {
            TableObject::TableName(name) => vec![table_name(name)?],
            _ => return Err(violation("table form not allowed")),
        },
        Statement::Update(update) => match &update.table.relation {
            TableFactor::Table { name, .. } => vec![table_name(name)?],
            _ => return Err(violation("table form not allowed")),
        },
        Statement::Delete(delete) => {
            let mut targets = delete
                .tables
                .iter()
                .map(table_name)
                .collect::<Result<Vec<_>, _>>()?;
            let from = match &delete.from {
                haex_crdt::sqlparser::ast::FromTable::WithFromKeyword(from)
                | haex_crdt::sqlparser::ast::FromTable::WithoutKeyword(from) => from,
            };
            for item in from {
                if let TableFactor::Table { name, .. } = &item.relation {
                    targets.push(table_name(name)?);
                }
            }
            targets
        }
        _ => Vec::new(),
    })
}

/// The lower-case names of every `WITH` table of a statement (for the authorizer, which sees a CTE
/// as an accessor). Collected without judging anything.
pub fn cte_names(statement: &Statement) -> HashSet<String> {
    struct Ctes(HashSet<String>);
    impl Visitor for Ctes {
        type Break = ();
        fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<()> {
            if let Some(with) = &query.with {
                for cte in &with.cte_tables {
                    self.0.insert(cte.alias.name.value.to_ascii_lowercase());
                }
            }
            ControlFlow::Continue(())
        }
    }
    let mut ctes = Ctes(HashSet::new());
    let _ = statement.visit(&mut ctes);
    ctes.0
}

/// Checks the identifiers of the SQL text: ASCII only (SQLite folds only ASCII case, so anything
/// else could name a table differently than this check sees it), and no sync column (`haex_*`).
pub(crate) fn check_words(sql: &str) -> Result<(), BridgeError> {
    for word in words(sql)? {
        if !word.is_ascii() {
            return Err(violation("identifiers must be ASCII"));
        }
        if word.to_ascii_lowercase().starts_with("haex_") {
            return Err(violation("sync columns and haex tables are not allowed"));
        }
    }
    Ok(())
}

/// Checks a run-time statement of the extension with the table prefix `own`. `existing` are the
/// lower-case names of the tables of the database; a `WITH` name may not be one of them, nor have
/// the form of an extension's table.
pub fn check(
    statement: &Statement,
    sql: &str,
    own: &TablePrefix,
    existing: &HashSet<String>,
) -> Result<RequiredAccess, BridgeError> {
    if !matches!(
        statement,
        Statement::Query(_) | Statement::Insert(_) | Statement::Update(_) | Statement::Delete(_)
    ) {
        return Err(violation("statement kind not allowed"));
    }
    check_words(sql)?;

    let mut collector = Collector::default();
    if let ControlFlow::Break(e) = statement.visit(&mut collector) {
        return Err(e);
    }
    // An extension's table name is refused whether it exists or not: the answer must not tell
    // whether another extension's table exists (FR-062).
    if collector
        .ctes
        .iter()
        .any(|cte| existing.contains(cte) || ExtensionTable::parse(cte).is_ok())
    {
        return Err(violation("a WITH name may not be the name of a table"));
    }

    let targets: HashSet<String> = write_targets(statement)?.into_iter().collect();
    let mut access = RequiredAccess::default();
    let mut seen = HashSet::new();
    for name in collector.relations {
        if collector.ctes.contains(&name)
            || collector.table_functions.contains(&name)
            || !seen.insert(name.clone())
        {
            continue;
        }
        let table = match classify(&name, own) {
            TableClass::Own(table) | TableClass::Foreign(table) => table,
            TableClass::Core => return Err(violation("table not allowed")),
        };
        if targets.contains(&name) {
            access.writes.push(table);
        } else {
            access.reads.push(table);
        }
    }
    Ok(access)
}

#[cfg(test)]
#[path = "ast_check_tests.rs"]
mod tests;

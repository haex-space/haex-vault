//! The authorizer of extension SQL (contracts/sql-policy.md §Authorizer): it judges what SQLite
//! really accesses while it prepares the statement, so it decides even where the pre-check could
//! be wrong. Built per statement from the [`SqlPolicy`].

use std::collections::HashSet;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use haex_crdt::{AuthAction, AuthContext, Authorization, SqlAuthorizer};

use super::migrate_rules::own_or_rebuild;
use super::policy::SqlPolicy;
use super::{function_allowed, TABLE_FUNCTIONS};
use crate::extensions::ids::TablePrefix;

fn allow(yes: bool) -> Authorization {
    if yes {
        Authorization::Allow
    } else {
        Authorization::Deny
    }
}

/// `z_dirty_<table>_(insert|update|delete)`: the change trigger haex-crdt keeps for `table`.
fn trigger_table(accessor: &str) -> Option<String> {
    let rest = accessor.to_ascii_lowercase();
    let rest = rest.strip_prefix("z_dirty_")?.to_owned();
    ["_insert", "_update", "_delete"]
        .iter()
        .find_map(|suffix| rest.strip_suffix(suffix).map(str::to_owned))
}

fn in_main(context: &AuthContext<'_>) -> bool {
    context
        .database_name
        .is_some_and(|db| db.eq_ignore_ascii_case("main"))
}

/// A table the statement reads no column of (`count(*)`, `SELECT 1`, `EXISTS`): SQLite reports it
/// once, with an empty column name and the schema as written, so none when the name is not
/// qualified (`main.` reads take the usual way). Only names in `main` can be meant: extensions
/// never qualify with another schema (pre-check) nor create or attach one.
fn tableless_read(context: &AuthContext<'_>) -> bool {
    context.database_name.is_none()
        && matches!(context.action, AuthAction::Read { column_name, .. } if column_name.is_empty())
}

/// Functions haex-crdt's change triggers call besides the allowlist.
const TRIGGER_FUNCTIONS: &[&str] = &["gen_uuid", "current_hlc"];

/// Whether a change trigger of `trigger_table` may run at all: the one of a table this statement
/// writes, or haex-crdt's own trigger on one of its tables (`haex_*`, e.g. `haex_deleted_rows`
/// when a delete leaves its marker), which fires only from an allowed trigger because writing a
/// `haex_*` table directly is denied.
fn trigger_allowed(trigger_table: &str, writable: &HashSet<String>) -> bool {
    writable.contains(trigger_table) || trigger_table.starts_with("haex_")
}

/// What a change trigger may do: its own table and haex-crdt's tables, nothing else.
fn trigger_action(context: &AuthContext<'_>, writable: &HashSet<String>) -> Authorization {
    let table_ok = |table: &str| {
        let table = table.to_ascii_lowercase();
        in_main(context) && (writable.contains(&table) || table.starts_with("haex_"))
    };
    allow(match context.action {
        AuthAction::Select => true,
        AuthAction::Function { function_name } => {
            function_allowed(function_name)
                || TRIGGER_FUNCTIONS.contains(&function_name.to_ascii_lowercase().as_str())
        }
        AuthAction::Read { table_name, .. }
        | AuthAction::Insert { table_name }
        | AuthAction::Update { table_name, .. }
        | AuthAction::Delete { table_name } => table_ok(table_name),
        _ => false,
    })
}

/// The run-time authorizer. `writable` are the lower-case names of the tables this statement may
/// write; `ctes` the lower-case names of its `WITH` tables.
///
/// SQLite reports the body of a `WITH` with the CTE's name as accessor and a read of a CTE with
/// no database: both are judged like top-level SQL, so the tables a CTE reads are checked, and a
/// CTE named like a change trigger gains nothing.
pub fn runtime(
    policy: Arc<SqlPolicy>,
    writable: HashSet<String>,
    ctes: HashSet<String>,
) -> SqlAuthorizer {
    Arc::new(move |context: &AuthContext<'_>| {
        let accessor = context
            .accessor
            .map(str::to_ascii_lowercase)
            .filter(|a| !ctes.contains(a));
        if let Some(accessor) = accessor {
            return match trigger_table(&accessor) {
                Some(table) if trigger_allowed(&table, &writable) => {
                    trigger_action(context, &writable)
                }
                _ => Authorization::Deny,
            };
        }
        match context.action {
            AuthAction::Select | AuthAction::Recursive => Authorization::Allow,
            AuthAction::Function { function_name } => allow(function_allowed(function_name)),
            // `json_each` and friends are eponymous tables; what their arguments read is
            // judged on its own. First, because `count(*)` over one comes without a database.
            AuthAction::Read { table_name, .. }
                if TABLE_FUNCTIONS.contains(&table_name.to_ascii_lowercase().as_str()) =>
            {
                Authorization::Allow
            }
            AuthAction::Read { table_name, .. } if context.database_name.is_none() => allow(
                ctes.contains(&table_name.to_ascii_lowercase())
                    || (tableless_read(context) && policy.allows(table_name, false)),
            ),
            AuthAction::Read { table_name, .. } => {
                allow(in_main(context) && policy.allows(table_name, false))
            }
            AuthAction::Insert { table_name }
            | AuthAction::Update { table_name, .. }
            | AuthAction::Delete { table_name } => allow(
                in_main(context)
                    && policy.allows(table_name, true)
                    && writable.contains(&table_name.to_ascii_lowercase()),
            ),
            _ => Authorization::Deny,
        }
    })
}

/// What the migration code is running at the moment; set by haex-vault's own code around each step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MigrationPhase {
    /// A checked `INSERT`/`UPDATE`/`DELETE`/query of the migration.
    Data = 0,
    /// A checked `CREATE`/`ALTER`/`DROP`: SQLite rewrites its schema tables meanwhile.
    Schema = 1,
    /// haex-vault writes the journal row of the migration.
    Journal = 2,
}

/// The shared phase of one migration run.
#[derive(Debug, Default)]
pub struct PhaseCell(AtomicU8);

impl PhaseCell {
    pub fn set(&self, phase: MigrationPhase) {
        self.0.store(phase as u8, Ordering::SeqCst);
    }

    fn get(&self) -> MigrationPhase {
        match self.0.load(Ordering::SeqCst) {
            1 => MigrationPhase::Schema,
            2 => MigrationPhase::Journal,
            _ => MigrationPhase::Data,
        }
    }
}

/// The journal table haex-vault writes after a migration (data-model.md).
pub const MIGRATION_JOURNAL: &str = "extension_migrations_applied_no_sync";

/// SQLite's own tables and functions it uses while it carries out a schema change.
const SCHEMA_TABLES: &[&str] = &[
    "sqlite_master",
    "sqlite_schema",
    "sqlite_sequence",
    "sqlite_temp_master",
];
const SCHEMA_FUNCTIONS: &[&str] = &[
    "sqlite_rename_table",
    "sqlite_rename_test",
    "sqlite_rename_column",
    "sqlite_rename_quotefix",
    "sqlite_drop_column",
];

/// The authorizer of migrations (contracts/sql-policy.md §Authorizer im Migrationsprofil): schema
/// changes and data of the own tables (and the `__new_` tables of a rebuild) only; SQLite's schema
/// tables only while a checked schema step runs; the journal only in the journal phase.
pub fn migration(own: TablePrefix, phase: Arc<PhaseCell>) -> SqlAuthorizer {
    Arc::new(move |context: &AuthContext<'_>| {
        let phase = phase.get();
        let mine = |table: &str| own_or_rebuild(table, &own);
        if phase == MigrationPhase::Journal {
            return allow(
                matches!(
                    context.action,
                    AuthAction::Insert { table_name } if table_name.eq_ignore_ascii_case(MIGRATION_JOURNAL)
                ) && in_main(context),
            );
        }
        let schema = phase == MigrationPhase::Schema;
        let schema_table =
            |table: &str| schema && SCHEMA_TABLES.contains(&table.to_ascii_lowercase().as_str());
        if let Some(accessor) = context.accessor {
            let table = trigger_table(accessor);
            let allowed = table.is_some_and(|t| mine(&t) || t.starts_with("haex_"));
            if !allowed {
                return Authorization::Deny;
            }
            return allow(match context.action {
                AuthAction::Select => true,
                AuthAction::Function { function_name } => {
                    function_allowed(function_name)
                        || TRIGGER_FUNCTIONS.contains(&function_name.to_ascii_lowercase().as_str())
                }
                AuthAction::Read { table_name, .. }
                | AuthAction::Insert { table_name }
                | AuthAction::Update { table_name, .. }
                | AuthAction::Delete { table_name } => {
                    in_main(context)
                        && (mine(table_name)
                            || table_name.to_ascii_lowercase().starts_with("haex_"))
                }
                _ => false,
            });
        }
        allow(match context.action {
            AuthAction::Select | AuthAction::Recursive => true,
            AuthAction::Function { function_name } => {
                function_allowed(function_name)
                    || (schema
                        && SCHEMA_FUNCTIONS.contains(&function_name.to_ascii_lowercase().as_str()))
            }
            AuthAction::CreateTable { table_name }
            | AuthAction::DropTable { table_name }
            | AuthAction::AlterTable { table_name, .. }
            | AuthAction::CreateIndex { table_name, .. }
            | AuthAction::DropIndex { table_name, .. } => schema && mine(table_name),
            AuthAction::DropTrigger {
                trigger_name,
                table_name,
            } => {
                schema
                    && mine(table_name)
                    && trigger_name.to_ascii_lowercase().starts_with("z_dirty_")
            }
            AuthAction::Reindex { .. } => schema,
            AuthAction::Read { table_name, .. }
                if TABLE_FUNCTIONS.contains(&table_name.to_ascii_lowercase().as_str()) =>
            {
                true
            }
            AuthAction::Read { table_name, .. } => {
                ((in_main(context) || tableless_read(context)) && mine(table_name))
                    || schema_table(table_name)
            }
            AuthAction::Insert { table_name }
            | AuthAction::Update { table_name, .. }
            | AuthAction::Delete { table_name } => {
                (in_main(context) && mine(table_name)) || schema_table(table_name)
            }
            _ => false,
        })
    })
}

#[cfg(test)]
#[path = "authorizer_tests.rs"]
mod tests;

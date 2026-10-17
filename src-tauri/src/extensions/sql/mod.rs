//! SQL of extensions (spec 017, US2, contracts/sql-policy.md, research R6–R8): a pre-check on the
//! syntax tree, ported from haex-vault 2.x (Legacy) and closed where it had gaps, and the SQLite authorizer of
//! haex-crdt's `SqlGuard` that decides on what SQLite really accesses. Both use the rules of this
//! module: which table is whose, and which functions exist for extensions.

pub mod ast_check;
pub mod authorizer;
pub mod changes;
pub mod exec;
pub mod migrate;
pub mod migrate_rules;
pub mod parse;
pub mod policy;
pub mod values;

use crate::extensions::ids::{ExtensionTable, TablePrefix};

/// Whose a table is, seen from the calling extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableClass {
    Own(ExtensionTable),
    /// A well-formed table of another extension, installed or not (FR-062: the same answer).
    Foreign(ExtensionTable),
    /// Everything else: holzi's tables, `sqlite_*`, `haex_*`, malformed names.
    Core,
}

/// Classifies a table name (as written or as SQLite reports it) for `own`.
pub fn classify(name: &str, own: &TablePrefix) -> TableClass {
    match ExtensionTable::parse(name) {
        Ok(table) if &table.prefix == own => TableClass::Own(table),
        Ok(table) => TableClass::Foreign(table),
        Err(_) => TableClass::Core,
    }
}

/// Table-valued functions an extension may use in `FROM`; they read only their arguments.
pub const TABLE_FUNCTIONS: &[&str] = &["json_each", "json_tree", "jsonb_each", "jsonb_tree"];

/// Scalar, aggregate and window functions an extension may call (contracts/sql-policy.md
/// §Erlaubte Funktionen). The one list for the pre-check and the authorizer. Never in it:
/// `load_extension`, `fts3_tokenizer`, `sqlite_*`, `sqlcipher_*`, `pragma_*`, application functions.
pub const FUNCTIONS: &[&str] = &[
    // Core functions.
    "abs",
    "char",
    "coalesce",
    "concat",
    "concat_ws",
    "format",
    "glob",
    "hex",
    "ifnull",
    "iif",
    "instr",
    "length",
    "like",
    "likelihood",
    "likely",
    "lower",
    "ltrim",
    "max",
    "min",
    "nullif",
    "octet_length",
    "printf",
    "quote",
    "random",
    "randomblob",
    "replace",
    "round",
    "rtrim",
    "sign",
    "substr",
    "substring",
    "trim",
    "typeof",
    "unhex",
    "unicode",
    "unlikely",
    "upper",
    "zeroblob",
    "changes",
    "last_insert_rowid",
    "total_changes",
    // Aggregates.
    "avg",
    "count",
    "group_concat",
    "string_agg",
    "sum",
    "total",
    // Window functions.
    "row_number",
    "rank",
    "dense_rank",
    "percent_rank",
    "cume_dist",
    "ntile",
    "lag",
    "lead",
    "first_value",
    "last_value",
    "nth_value",
    // Date and time.
    "date",
    "time",
    "datetime",
    "julianday",
    "unixepoch",
    "strftime",
    "timediff",
    // The keywords `CURRENT_DATE`, `CURRENT_TIME` and `CURRENT_TIMESTAMP` (Drizzle writes the last
    // as a column default); they mean `date('now')`, `time('now')` and `datetime('now')`.
    "current_date",
    "current_time",
    "current_timestamp",
    // Math.
    "acos",
    "acosh",
    "asin",
    "asinh",
    "atan",
    "atan2",
    "atanh",
    "ceil",
    "ceiling",
    "cos",
    "cosh",
    "degrees",
    "exp",
    "floor",
    "ln",
    "log",
    "log10",
    "log2",
    "mod",
    "pi",
    "pow",
    "power",
    "radians",
    "sin",
    "sinh",
    "sqrt",
    "tan",
    "tanh",
    "trunc",
    // JSON (the `->`/`->>` operators reach the authorizer as functions of those names).
    "json",
    "jsonb",
    "json_array",
    "jsonb_array",
    "json_array_length",
    "json_error_position",
    "json_extract",
    "jsonb_extract",
    "json_insert",
    "jsonb_insert",
    "json_object",
    "jsonb_object",
    "json_patch",
    "jsonb_patch",
    "json_pretty",
    "json_remove",
    "jsonb_remove",
    "json_replace",
    "jsonb_replace",
    "json_set",
    "jsonb_set",
    "json_type",
    "json_valid",
    "json_quote",
    "json_group_array",
    "jsonb_group_array",
    "json_group_object",
    "jsonb_group_object",
    "->",
    "->>",
];

/// Whether an extension may call the function `name` (any case).
pub fn function_allowed(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    FUNCTIONS.contains(&name.as_str()) || TABLE_FUNCTIONS.contains(&name.as_str())
}

#[cfg(test)]
mod bypass_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

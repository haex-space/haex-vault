//! The database methods of the bridge (US2, T061): `extension_database_query`, `_execute`,
//! `_transaction` and `_register_migrations`. Every call runs as the calling frame's extension,
//! with its limits and at most `max_concurrent` at a time.

use std::collections::HashSet;
use std::sync::{Arc, PoisonError};

use serde_json::{json, Value};
use uuid::Uuid;

use super::dispatch::CallContext;
use super::frames::FrameSource;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::host::{ExtensionHost, SqlSlot};
use crate::extensions::ids::{ExtensionName, PublicKey, TablePrefix};
use crate::extensions::permissions::store::candidates;
use crate::extensions::permissions::PermissionKind;
use crate::extensions::registry::start::{effective, migrate, Effective};
use crate::extensions::sql::exec::{
    existing_tables, limits_of, prepare, run_in, Limits, SqlResult,
};
use crate::extensions::sql::migrate::{apply_pending_local, applying, MigrationError, Tables};
use crate::extensions::sql::policy::SqlPolicy;
use crate::extensions::sql::values::{params, statement_entry};
use crate::passwords::clock::unix_millis;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

pub const MODULE: &str = module_path!();

fn unavailable() -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Database, "database unavailable")
}

fn invalid(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

/// The calling extension's table prefix, from its registry row.
fn own_prefix(ctx: &CallContext) -> Result<TablePrefix, BridgeError> {
    prefix_of(&ctx.db, ctx.session.extension_id)
}

/// An extension's table prefix, from its registry row or, for a development version, its row on
/// this device (the ids come from different namespaces, so at most one matches).
fn prefix_of(db: &VaultDb, extension_id: Uuid) -> Result<TablePrefix, BridgeError> {
    let id = extension_id.to_string();
    let (key, name) = db
        .read_blocking(move |q| {
            q.query_row(
                "SELECT public_key, name FROM extensions WHERE id = ?1 \
                 UNION ALL SELECT public_key, name FROM dev_extensions_no_sync WHERE id = ?1",
                &[&id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
        })
        .map_err(|_| unavailable())?
        .ok_or_else(|| BridgeError::new(ExtensionErrorCode::NotFound, "not found"))?;
    Ok(TablePrefix {
        public_key: PublicKey::parse(&key).map_err(|_| unavailable())?,
        name: ExtensionName::parse(&name).map_err(|_| unavailable())?,
    })
}

/// Where the caller's own tables live: a development version's on this device only (US12).
fn tables_of(ctx: &CallContext) -> Tables {
    match ctx.session.source {
        FrameSource::Bundle(_) => Tables::Synced,
        FrameSource::DevServer => Tables::DeviceLocal,
    }
}

/// What every SQL call needs: the policy, the limits and a running slot.
struct SqlCall {
    policy: Arc<SqlPolicy>,
    limits: Limits,
    _slot: SqlSlot,
}

/// The SQL policy of the calling extension: its prefix, its remembered `database` permissions for
/// this device and the decisions held in memory (read fresh on every call, so a change applies
/// to the next call of an open frame).
pub fn policy(ctx: &CallContext) -> Result<SqlPolicy, BridgeError> {
    policy_for(&ctx.db, &ctx.host, ctx.session.extension_id, ctx.device)
}

/// The table prefixes of the installed extensions; a row haex-vault cannot read is left out.
fn installed_prefixes(q: &mut impl Query) -> haex_crdt::Result<HashSet<TablePrefix>> {
    let rows = q.query_map(
        "SELECT public_key, name FROM extensions WHERE state = 'installed'",
        &[],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|(key, name)| {
            Some(TablePrefix {
                public_key: PublicKey::parse_case_insensitive(&key).ok()?,
                name: ExtensionName::parse(&name).ok()?,
            })
        })
        .collect())
}

/// The SQL policy of `extension_id` on `device` now: its remembered and temporary `database`
/// permissions and which extensions are installed. The change notifications filter with the same
/// policy (research R9).
pub fn policy_for(
    db: &VaultDb,
    host: &ExtensionHost,
    extension_id: Uuid,
    device: Uuid,
) -> Result<SqlPolicy, BridgeError> {
    let (mut grants, installed) = db
        .read_blocking(move |q| {
            let grants = candidates(q, extension_id, PermissionKind::Database, device)
                .map_err(haex_crdt::Error::from)?;
            Ok((grants, installed_prefixes(q)?))
        })
        .map_err(|_| unavailable())?;
    grants.extend(
        host.permissions
            .temporary(extension_id, PermissionKind::Database),
    );
    Ok(SqlPolicy {
        own: prefix_of(db, extension_id)?,
        grants,
        device,
        installed,
    })
}

fn sql_call(ctx: &CallContext) -> Result<SqlCall, BridgeError> {
    let extension_id = ctx.session.extension_id;
    let limits = ctx
        .db
        .read_blocking(move |q| limits_of(q, extension_id))
        .map_err(|_| unavailable())?;
    let slot = ctx
        .host
        .enter_sql(extension_id, limits.max_concurrent)
        .ok_or_else(|| BridgeError::new(ExtensionErrorCode::LimitExceeded, "too many requests"))?;
    Ok(SqlCall {
        policy: Arc::new(policy(ctx)?),
        limits,
        _slot: slot,
    })
}

fn result_json(result: SqlResult) -> Result<Value, BridgeError> {
    serde_json::to_value(result).map_err(|_| unavailable())
}

/// `{sql | query, params}`: one statement.
fn one_statement(ctx: &CallContext, call_params: &Value) -> Result<Value, BridgeError> {
    let sql = call_params
        .get("sql")
        .or_else(|| call_params.get("query"))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("sql is missing"))?;
    let call = sql_call(ctx)?;
    let existing = existing_tables(&ctx.db)?;
    let checked = prepare(
        sql,
        params(call_params.get("params"))?,
        &call.policy,
        &call.limits,
        &existing,
    )?;
    result_json(run_in(
        &ctx.db,
        Arc::clone(&call.policy),
        &call.limits,
        vec![checked],
        tables_of(ctx),
    )?)
}

pub fn query(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    one_statement(ctx, params)
}

pub fn execute(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    one_statement(ctx, params)
}

/// `{statements: [[sql, params], …]}`: all or nothing.
pub fn transaction(ctx: &CallContext, call_params: &Value) -> Result<Value, BridgeError> {
    let entries = call_params
        .get("statements")
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("statements must be a non-empty array"))?;
    let call = sql_call(ctx)?;
    let existing = existing_tables(&ctx.db)?;
    let checked = entries
        .iter()
        .map(|entry| {
            let (sql, entry_params) = statement_entry(entry)?;
            prepare(
                sql,
                params(entry_params)?,
                &call.policy,
                &call.limits,
                &existing,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    result_json(run_in(
        &ctx.db,
        Arc::clone(&call.policy),
        &call.limits,
        checked,
        tables_of(ctx),
    )?)
}

/// `{extensionVersion, migrations: [{name, sql}]}`: only migrations of the verified effective
/// bundle are accepted (same name and SQL); its pending ones are applied (FR-003). Answers the
/// SDK's `MigrationResult`.
pub fn register_migrations(ctx: &CallContext, call_params: &Value) -> Result<Value, BridgeError> {
    let migrations = call_params
        .get("migrations")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("migrations must be an array"))?;
    let extension_id = ctx.session.extension_id;
    if ctx.session.source == FrameSource::DevServer {
        return register_dev_migrations(ctx, migrations);
    }
    let Effective::Ready(prepared) = effective(&ctx.db, extension_id).map_err(|_| unavailable())?
    else {
        return Err(unavailable());
    };
    for migration in migrations {
        let name = migration.get("name").and_then(Value::as_str);
        let sql = migration.get("sql").and_then(Value::as_str);
        let (Some(name), Some(sql)) = (name, sql) else {
            return Err(invalid("a migration needs name and sql"));
        };
        if !prepared
            .migrations
            .iter()
            .any(|(n, s)| n == name && s == sql)
        {
            return Err(BridgeError::new(
                ExtensionErrorCode::SecurityViolation,
                "migration is not part of the installed bundle",
            ));
        }
    }
    let applied = migrate(
        &ctx.db,
        extension_id,
        &prepared,
        unix_millis(std::time::SystemTime::now()),
    )
    .map_err(migration_error)?;
    Ok(json!({
        "appliedCount": applied.len(),
        "alreadyAppliedCount": migrations.len().saturating_sub(applied.len()),
        "appliedMigrations": applied,
    }))
}

/// The SQL error of a migration as the bridge answers it.
fn migration_error(error: MigrationError) -> BridgeError {
    match error {
        MigrationError::Refused { error, .. } => *error,
        MigrationError::Unavailable => unavailable(),
        _ => BridgeError::new(ExtensionErrorCode::Database, "migration failed"),
    }
}

/// A development version has no signed bundle: the migrations it registers are its own, checked
/// with the same rules and created on this device only (US12, research R16).
fn register_dev_migrations(ctx: &CallContext, migrations: &[Value]) -> Result<Value, BridgeError> {
    let mut offered = Vec::with_capacity(migrations.len());
    for migration in migrations {
        let name = migration.get("name").and_then(Value::as_str);
        let sql = migration.get("sql").and_then(Value::as_str);
        let (Some(name), Some(sql)) = (name, sql) else {
            return Err(invalid("a migration needs name and sql"));
        };
        offered.push((name.to_owned(), sql.to_owned()));
    }
    let applying = applying().lock().unwrap_or_else(PoisonError::into_inner);
    // Read under the lock: after an unload the registration is gone and nothing is created.
    let own = own_prefix(ctx)?;
    let applied = apply_pending_local(
        &applying,
        &ctx.db,
        ctx.session.extension_id,
        &own,
        &offered,
        unix_millis(std::time::SystemTime::now()),
    )
    .map_err(migration_error)?;
    Ok(json!({
        "appliedCount": applied.len(),
        "alreadyAppliedCount": migrations.len().saturating_sub(applied.len()),
        "appliedMigrations": applied,
    }))
}

#[cfg(test)]
#[path = "foreign_tables_tests.rs"]
mod foreign_tables_tests;

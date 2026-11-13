//! The default model of an extension (spec 055 FR-007): at most one of the models it may use, to
//! which its jobs go first. There is no rank; when the default cannot take a job, holzi asks the
//! user which other model should (`model_jobs::preflight`, `model_jobs::runner`).
//!
//! The row lives in `extension_default_model`, a vault-wide table that syncs; its id is the
//! extension's id, so two devices that set the default write the same row. Writes are
//! check-then-write like the other CRDT tables.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::{PermissionKind, Target};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

/// The default model of `extension_id`, when it has one.
pub fn get(q: &mut impl Query, extension_id: Uuid) -> Result<Option<String>> {
    Ok(q.query_map(
        "SELECT model_id FROM extension_default_model WHERE extension_id = ?1",
        &[&extension_id.to_string()],
        |r| r.get::<_, String>(0),
    )?
    .into_iter()
    .next())
}

fn installed(tx: &mut CrdtTransaction<'_>, extension_id: Uuid) -> Result<bool> {
    Ok(tx
        .query_row(
            "SELECT COUNT(*) FROM extensions WHERE id = ?1 AND state = 'installed'",
            params![extension_id.to_string()],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0)
}

/// A model id the permission kind `model` accepts, and not the wildcard.
fn checked(model_id: &str) -> Result<()> {
    match Target::parse(PermissionKind::Model, model_id) {
        Some(Target::Model(_)) => Ok(()),
        _ => Err(HolziError::InvalidInput {
            reason: "not a model id".into(),
        }),
    }
}

/// Makes `model_id` the default of an installed extension, in place of the one it had.
pub fn set(tx: &mut CrdtTransaction<'_>, extension_id: Uuid, model_id: &str) -> Result<()> {
    checked(model_id)?;
    if !installed(tx, extension_id)? {
        return Err(HolziError::ExtensionNotFound);
    }
    let id = extension_id.to_string();
    let known = tx
        .query_row(
            "SELECT COUNT(*) FROM extension_default_model WHERE id = ?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    if known {
        tx.execute(
            "UPDATE extension_default_model SET model_id = ?2 WHERE id = ?1",
            params![id, model_id],
        )?;
    } else {
        tx.execute(
            "INSERT INTO extension_default_model (id, extension_id, model_id) VALUES (?1, ?1, ?2)",
            params![id, model_id],
        )?;
    }
    Ok(())
}

/// Takes the default away: the extension has none.
pub fn clear(tx: &mut CrdtTransaction<'_>, extension_id: Uuid) -> Result<()> {
    tx.execute(
        "DELETE FROM extension_default_model WHERE id = ?1",
        params![extension_id.to_string()],
    )?;
    Ok(())
}

/// Takes the default away when it is `model_id`: a model the extension may no longer use cannot
/// stay its default.
pub fn clear_if(tx: &mut CrdtTransaction<'_>, extension_id: Uuid, model_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM extension_default_model WHERE id = ?1 AND model_id = ?2",
        params![extension_id.to_string(), model_id],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "default_model_tests.rs"]
mod tests;

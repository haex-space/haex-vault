//! The rank of the models an extension may use (spec 055 FR-007, research R5): the user orders
//! them per extension, and holzi tries a step's candidates in that order. A grant does not carry
//! an order, so it lives in `extension_model_order`, a vault-wide table that syncs.
//!
//! A row exists per ranked model; a model without a row comes after every ranked one, in the order
//! of its grant (the caller sorts those). Rows of models that are no longer granted are harmless
//! and ignored by the caller. Writes are check-then-write like the other CRDT tables.

use std::collections::HashSet;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::{PermissionKind, Target};
use crate::error::{HolziError, Result};
use crate::extensions::ids::model_order_id;
use crate::storage::query::Query;

/// The most models one extension can rank.
pub const MAX_RANKED: usize = 1000;

/// The ranked models of `extension_id`, first choice first. Two rows with the same position (two
/// devices appended at once) come in the order of the model id, so every device reads one order.
pub fn ordered(q: &mut impl Query, extension_id: Uuid) -> Result<Vec<String>> {
    Ok(q.query_map(
        "SELECT model_id FROM extension_model_order WHERE extension_id = ?1 \
         ORDER BY position, model_id",
        &[&extension_id.to_string()],
        |r| r.get::<_, String>(0),
    )?)
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

/// Puts `model_id` at the end of the rank unless it is ranked already: a new grant comes last.
pub fn append(tx: &mut CrdtTransaction<'_>, extension_id: Uuid, model_id: &str) -> Result<()> {
    checked(model_id)?;
    if !installed(tx, extension_id)? {
        return Err(HolziError::ExtensionNotFound);
    }
    let id = model_order_id(extension_id, model_id).to_string();
    let known = tx
        .query_row(
            "SELECT COUNT(*) FROM extension_model_order WHERE id = ?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    if known {
        return Ok(());
    }
    let last = tx
        .query_row(
            "SELECT MAX(position) FROM extension_model_order WHERE extension_id = ?1",
            params![extension_id.to_string()],
            |r| r.get::<_, Option<i64>>(0),
        )?
        .flatten();
    tx.execute(
        "INSERT INTO extension_model_order (id, extension_id, model_id, position) \
         VALUES (?1, ?2, ?3, ?4)",
        params![
            id,
            extension_id.to_string(),
            model_id,
            last.map_or(0, |n| n + 1)
        ],
    )?;
    Ok(())
}

/// Replaces the whole rank of `extension_id` by `model_ids`, first choice first. A repeated id or
/// a list longer than [`MAX_RANKED`] is refused and nothing is written; a model that was ranked and
/// is not listed loses its place.
pub fn set_order(
    tx: &mut CrdtTransaction<'_>,
    extension_id: Uuid,
    model_ids: &[String],
) -> Result<()> {
    if model_ids.len() > MAX_RANKED {
        return Err(HolziError::InvalidInput {
            reason: format!("at most {MAX_RANKED} models can be ranked"),
        });
    }
    let mut seen = HashSet::new();
    for id in model_ids {
        checked(id)?;
        if !seen.insert(id.as_str()) {
            return Err(HolziError::InvalidInput {
                reason: "a model is listed twice".into(),
            });
        }
    }
    if !installed(tx, extension_id)? {
        return Err(HolziError::ExtensionNotFound);
    }
    let ext = extension_id.to_string();
    let keep: HashSet<String> = model_ids
        .iter()
        .map(|id| model_order_id(extension_id, id).to_string())
        .collect();
    let existing: Vec<String> = tx.query_map(
        "SELECT id FROM extension_model_order WHERE extension_id = ?1",
        params![ext],
        |r| r.get(0),
    )?;
    for id in existing.iter().filter(|id| !keep.contains(*id)) {
        tx.execute(
            "DELETE FROM extension_model_order WHERE id = ?1",
            params![id],
        )?;
    }
    for (position, model_id) in model_ids.iter().enumerate() {
        let id = model_order_id(extension_id, model_id).to_string();
        let position = i64::try_from(position).unwrap_or(i64::MAX);
        if existing.contains(&id) {
            tx.execute(
                "UPDATE extension_model_order SET position = ?2 WHERE id = ?1",
                params![id, position],
            )?;
        } else {
            tx.execute(
                "INSERT INTO extension_model_order (id, extension_id, model_id, position) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, ext, model_id, position],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;

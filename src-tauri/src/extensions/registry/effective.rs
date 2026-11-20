//! The effective bundle of an extension (research R11): the highest semver among its bundles with
//! `retired = 0`, on a tie the larger id. Every device computes the same answer from the same rows,
//! so no column has to name the current bundle (a last-writer-wins column would let an older
//! version win).

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use crate::error::Result;
use crate::storage::query::Query;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveBundle {
    pub bundle_id: Uuid,
    pub version: semver::Version,
}

/// Every bundle of the extension that is not retired, with its version. Rows whose version or id
/// does not parse are skipped (they cannot have been written by haex-vault).
pub fn live_bundles(q: &mut impl Query, extension_id: Uuid) -> Result<Vec<EffectiveBundle>> {
    let rows = q.query_map(
        "SELECT id, version FROM extension_bundles WHERE extension_id = ?1 AND retired = 0",
        &[&extension_id.to_string()],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, version)| {
            Some(EffectiveBundle {
                bundle_id: Uuid::parse_str(&id).ok()?,
                version: semver::Version::parse(&version).ok()?,
            })
        })
        .collect())
}

/// The effective bundle, or `None` when the extension has no live bundle.
pub fn effective_bundle(q: &mut impl Query, extension_id: Uuid) -> Result<Option<EffectiveBundle>> {
    Ok(live_bundles(q, extension_id)?.into_iter().max_by(|a, b| {
        a.version
            .cmp(&b.version)
            .then_with(|| a.bundle_id.to_string().cmp(&b.bundle_id.to_string()))
    }))
}

/// The retired bundles of a higher version than `effective`, lowest first. Confirming a downgrade
/// retires every higher bundle (research R11), so `effective` stands after one when this is not
/// empty.
pub fn retired_above(
    q: &mut impl Query,
    extension_id: Uuid,
    effective: &EffectiveBundle,
) -> Result<Vec<EffectiveBundle>> {
    let rows = q.query_map(
        "SELECT id, version FROM extension_bundles WHERE extension_id = ?1 AND retired = 1",
        &[&extension_id.to_string()],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?;
    let mut above: Vec<EffectiveBundle> = rows
        .into_iter()
        .filter_map(|(id, version)| {
            Some(EffectiveBundle {
                bundle_id: Uuid::parse_str(&id).ok()?,
                version: semver::Version::parse(&version).ok()?,
            })
        })
        .filter(|bundle| bundle.version > effective.version)
        .collect();
    above.sort_by(|a, b| {
        a.version
            .cmp(&b.version)
            .then_with(|| a.bundle_id.to_string().cmp(&b.bundle_id.to_string()))
    });
    Ok(above)
}

/// The other live bundles of `version`: same version, other content. On a tie the larger id would
/// win, so installing over one of them needs a confirmation and then retires them.
pub fn same_version_others(
    q: &mut impl Query,
    extension_id: Uuid,
    new_bundle: Uuid,
    version: &semver::Version,
) -> Result<Vec<EffectiveBundle>> {
    Ok(live_bundles(q, extension_id)?
        .into_iter()
        .filter(|b| &b.version == version && b.bundle_id != new_bundle)
        .collect())
}

/// Sets `retired = 1` on `bundles` (a confirmed downgrade or replacement, R11).
pub fn retire(tx: &mut CrdtTransaction<'_>, bundles: &[EffectiveBundle]) -> Result<()> {
    for bundle in bundles {
        tx.execute(
            "UPDATE extension_bundles SET retired = 1 WHERE id = ?1",
            params![bundle.bundle_id.to_string()],
        )?;
    }
    Ok(())
}

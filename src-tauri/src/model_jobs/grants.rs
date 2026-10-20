//! The user's grants of models to an extension (spec 055 US2, T032, T034): what the settings list
//! and change, and which models the question of a job offers.
//!
//! A model has at most one row of the kind `model` per extension: a grant with its ceiling, or a
//! denial. Writing one replaces every earlier row about the model, since a row of another effort
//! would stay and a denial would win over a new grant (`evaluate`).

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::candidates::{ceiling, granted, job_models, model_permissions, takes_images};
use super::effort::Level;
use super::job::Need;
use super::model_source::JobModel;
use crate::error::{HolziError, Result};
use crate::extensions::host::ExtensionHost;
use crate::extensions::permissions::store::{self, NewPermission};
use crate::extensions::permissions::{
    order, Permission, PermissionKind, PermissionStatus, Target, VAULT_WIDE,
};
use crate::storage::query::Query;

/// One model of the vault as the settings of an extension show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionModelEntry {
    /// `models.id`.
    pub id: String,
    pub name: String,
    /// The display name of the provider.
    pub provider: String,
    /// `this_device`, `network` or `cloud`.
    pub place: String,
    pub takes_images: bool,
    /// The highest effort allowed (`none`, `low`, `medium`, `high`); absent when the extension
    /// may not use the model.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ceiling: Option<String>,
    /// The user refused the model to the extension.
    pub denied: bool,
    /// Allowed only for this vault session, not remembered.
    pub held: bool,
}

/// One model the question of a job offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionModelChoice {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub place: String,
    pub takes_images: bool,
}

fn place_of(model: &JobModel) -> String {
    serde_json::to_value(model.place)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn denies(permissions: &[Permission], model_id: &str) -> bool {
    permissions.iter().any(|p| {
        p.status == PermissionStatus::Denied
            && match &p.target {
                Target::Any => true,
                Target::Model(id) => id == model_id,
                _ => false,
            }
    })
}

/// Every model of the vault for the settings of `extension_id`: those it may use first, in its
/// rank, then the others by name.
pub(crate) fn entries(
    q: &mut impl Query,
    extension_id: Uuid,
    device: Uuid,
    held: Vec<Permission>,
) -> Result<Vec<ExtensionModelEntry>> {
    let stored = model_permissions(q, extension_id, device, Vec::new())?;
    let mut all = stored.clone();
    all.extend(held);
    let entry = |model: &JobModel, allowed: Option<Level>| ExtensionModelEntry {
        id: model.id.clone(),
        name: model.name.clone(),
        provider: model.provider.name.clone(),
        place: place_of(model),
        takes_images: takes_images(model.capabilities.as_ref()),
        ceiling: allowed.map(|level| level.as_str().to_owned()),
        denied: denies(&all, &model.id),
        held: allowed.is_some() && ceiling(&stored, &model.id, device).is_none(),
    };
    let first = granted(q, extension_id, device, &all)?;
    let mut entries: Vec<ExtensionModelEntry> = first
        .iter()
        .map(|c| entry(&c.model, Some(c.ceiling)))
        .collect();
    let mut others: Vec<JobModel> = job_models(q)?
        .into_iter()
        .filter(|m| !first.iter().any(|c| c.model.id == m.id))
        .collect();
    others.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    entries.extend(others.iter().map(|m| entry(m, None)));
    Ok(entries)
}

/// The models a question offers for a step that needs `needs`: those that fit, `proposed` first,
/// then by name.
pub(crate) fn choices(
    q: &mut impl Query,
    needs: &[Need],
    proposed: &str,
) -> Result<Vec<ExtensionModelChoice>> {
    let mut fitting: Vec<JobModel> = job_models(q)?
        .into_iter()
        .filter(|m| super::candidates::fits(needs, m.capabilities.as_ref()))
        .collect();
    fitting.sort_by(|a, b| {
        (a.id != proposed, &a.name, &a.id).cmp(&(b.id != proposed, &b.name, &b.id))
    });
    Ok(fitting
        .iter()
        .map(|m| ExtensionModelChoice {
            id: m.id.clone(),
            name: m.name.clone(),
            provider: m.provider.name.clone(),
            place: place_of(m),
            takes_images: takes_images(m.capabilities.as_ref()),
        })
        .collect())
}

/// A model id the kind `model` accepts, not the wildcard.
fn checked(model_id: &str) -> Result<()> {
    match Target::parse(PermissionKind::Model, model_id) {
        Some(Target::Model(_)) => Ok(()),
        _ => Err(HolziError::InvalidInput {
            reason: "not a model id".into(),
        }),
    }
}

fn is_development(tx: &mut CrdtTransaction<'_>, extension_id: Uuid) -> Result<bool> {
    Ok(tx
        .query_row(
            "SELECT COUNT(*) FROM dev_extensions_no_sync WHERE id = ?1",
            params![extension_id.to_string()],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0)
}

/// Takes every remembered row about `model_id` away: the model is back to "ask".
pub(crate) fn revoke(
    tx: &mut CrdtTransaction<'_>,
    extension_id: Uuid,
    model_id: &str,
) -> Result<()> {
    checked(model_id)?;
    let about: Vec<Uuid> = store::rows_of(tx, extension_id)?
        .into_iter()
        .filter(|r| r.kind == PermissionKind::Model.as_str() && r.target == model_id)
        .map(|r| r.id)
        .collect();
    for id in about {
        store::delete(tx, id)?;
    }
    Ok(())
}

/// Remembers the user's decision about `model_id` for every own device: a grant with the ceiling
/// `effort`, or a denial. A new grant comes last in the rank; a development version has no rank
/// (data-model.md).
pub(crate) fn put(
    tx: &mut CrdtTransaction<'_>,
    extension_id: Uuid,
    model_id: &str,
    status: PermissionStatus,
    effort: Level,
    now_ms: i64,
) -> Result<()> {
    revoke(tx, extension_id, model_id)?;
    store::put(
        tx,
        extension_id,
        &NewPermission {
            kind: PermissionKind::Model.as_str(),
            action: effort.as_str(),
            target: model_id,
            status: status.as_str(),
            declared: false,
            vault_device_uuid: VAULT_WIDE,
        },
        now_ms,
    )?;
    if status == PermissionStatus::Granted && !is_development(tx, extension_id)? {
        order::append(tx, extension_id, model_id)?;
    }
    Ok(())
}

/// Forgets the decisions about `model_id` held for this vault session.
pub(crate) fn forget_held(host: &ExtensionHost, extension_id: Uuid, model_id: &str) {
    for decision in host.permissions.held(extension_id) {
        if decision.kind == PermissionKind::Model && decision.target == model_id {
            host.permissions.forget(
                extension_id,
                decision.kind,
                &decision.action,
                &decision.target,
            );
        }
    }
}

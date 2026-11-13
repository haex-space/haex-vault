//! The models a step may go to (spec 055 R4, R5, T025): the models the extension may use, with the
//! highest effort the user allowed, that can do what the step needs, the default model first. Whether a
//! model can be reached is not looked at here: an unreachable one stays a candidate, is tried, and
//! shows up as `unavailable` in the provenance while the next one takes over.

use std::collections::HashMap;

use uuid::Uuid;

use super::effort::Level;
use super::job::{Need, Step};
use super::model_source::JobModel;
use crate::adapters::AttachmentKind;
use crate::error::Result;
use crate::extensions::permissions::store;
use crate::extensions::permissions::{
    default_model, evaluate, Action, Decision, Permission, PermissionKind, PermissionRequest,
    RequestTarget,
};
use crate::model_capabilities::ModelCapabilities;
use crate::storage::models::list_all_models;
use crate::storage::providers::{list_providers, Provider, ProviderCapability, ProviderKind};
use crate::storage::query::Query;

/// A model a step may go to, and the effort the user chose for it (stored as the highest effort
/// the grant covers, which is why it is called the ceiling).
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub model: JobModel,
    pub ceiling: Level,
}

/// The models of the vault that can take a job: the chat models of every provider, whatever their
/// source (spec 055 FR-031): a local one, one with an address, a connected subscription.
pub(crate) fn job_models(q: &mut impl Query) -> Result<Vec<JobModel>> {
    let providers: HashMap<Uuid, Provider> = list_providers(q)?
        .into_iter()
        .filter(|p| p.capability == ProviderCapability::Chat)
        .map(|p| (p.id, p))
        .collect();
    Ok(list_all_models(q)?
        .into_iter()
        .filter_map(|model| {
            let provider = providers.get(&model.provider_id)?.clone();
            Some(JobModel::new(model, provider))
        })
        .collect())
}

/// The effort `permissions` allow for `model_id`, which is the one the user chose for it, or `None`
/// when the model may not be used. A grant covers every effort up to its own (`Action::covers`):
/// a denial of the model, whatever its effort, covers `none` and so takes the model away entirely.
pub(crate) fn ceiling(permissions: &[Permission], model_id: &str, device: Uuid) -> Option<Level> {
    Level::ALL
        .into_iter()
        .take_while(|level| {
            evaluate(permissions, &model_request(*level, model_id), device) == Decision::Allow
        })
        .last()
}

/// The question whether `model_id` may be used with `level`.
pub(crate) fn model_request(level: Level, model_id: &str) -> PermissionRequest {
    PermissionRequest {
        kind: PermissionKind::Model,
        action: Action::Effort(level),
        target: RequestTarget::Model(model_id.to_owned()),
    }
}

/// When each model was granted (`*` for the grant of every model): the order of the models that
/// are not the default.
fn grant_times(q: &mut impl Query, extension_id: Uuid) -> Result<HashMap<String, i64>> {
    let rows = q.query_map(
        "SELECT target, MIN(updated_at) FROM ( \
           SELECT target, updated_at FROM extension_permissions \
             WHERE extension_id = ?1 AND kind = 'model' AND status = 'granted' \
           UNION ALL \
           SELECT target, updated_at FROM dev_extension_permissions_no_sync \
             WHERE extension_id = ?1 AND kind = 'model' AND status = 'granted') \
         GROUP BY target",
        &[&extension_id.to_string()],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
    )?;
    Ok(rows.into_iter().collect())
}

/// The model permissions of the extension on `device`: remembered ones and those `held` for the
/// session.
pub(crate) fn model_permissions(
    q: &mut impl Query,
    extension_id: Uuid,
    device: Uuid,
    held: Vec<Permission>,
) -> Result<Vec<Permission>> {
    let mut permissions = store::candidates(q, extension_id, PermissionKind::Model, device)?;
    permissions.extend(held);
    Ok(permissions)
}

/// Every model the extension may use, first choice first: the default model, then the others in the
/// order they were granted (one held for the session last), then by id.
pub(crate) fn granted(
    q: &mut impl Query,
    extension_id: Uuid,
    device: Uuid,
    permissions: &[Permission],
) -> Result<Vec<Candidate>> {
    let default = default_model::get(q, extension_id)?;
    let times = grant_times(q, extension_id)?;
    let mut sorted: Vec<(bool, i64, Candidate)> = job_models(q)?
        .into_iter()
        .filter_map(|model| {
            let ceiling = ceiling(permissions, &model.id, device)?;
            let not_default = default.as_deref() != Some(model.id.as_str());
            let at = times
                .get(&model.id)
                .or_else(|| times.get("*"))
                .copied()
                .unwrap_or(i64::MAX);
            Some((not_default, at, Candidate { model, ceiling }))
        })
        .collect();
    sorted.sort_by(
        |(not_default, at, left), (other_not_default, other_at, right)| {
            (not_default, at, &left.model.id).cmp(&(other_not_default, other_at, &right.model.id))
        },
    );
    Ok(sorted
        .into_iter()
        .map(|(_, _, candidate)| candidate)
        .collect())
}

/// Whether the model takes images; a model nobody knows about does not (FR-019).
pub(crate) fn takes_images(capabilities: Option<&ModelCapabilities>) -> bool {
    capabilities
        .and_then(|c| c.accepted_attachment_kinds.as_ref())
        .is_some_and(|kinds| kinds.contains(&AttachmentKind::Image))
}

/// Whether a model can do what a step needs. Every chat model takes text.
pub(crate) fn fits(needs: &[Need], model: &JobModel) -> bool {
    if !needs.contains(&Need::Image) {
        return true;
    }

    // Codex receives only the transcript text today; unlike Claude, its adapter does not write
    // the request's attachment bytes into the invocation workspace.
    if model.provider.kind == ProviderKind::CliDelegate
        && model.provider.adapter.as_deref() == Some("codex")
    {
        return false;
    }

    takes_images(model.capabilities.as_ref())
}

/// The candidates of one step, in the order of `granted`.
pub(crate) fn for_step(granted: &[Candidate], step: &Step) -> Vec<Candidate> {
    granted
        .iter()
        .filter(|c| fits(&step.needs, &c.model))
        .cloned()
        .collect()
}

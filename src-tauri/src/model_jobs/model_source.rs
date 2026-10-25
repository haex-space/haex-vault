//! Where the adapter of a job's model comes from (spec 055 T023): a model the extension may use is
//! read from its `models` and `providers` rows once, before the run; the adapter is built only when
//! the model is tried, so a broken provider shows up as `unavailable` in the provenance and the
//! next model can take over.
//!
//! The source is a trait so tests can hand in a scripted adapter; holzi itself uses
//! [`Providers`]. Local models take jobs once they can read images (Phase 8); until then they are
//! unavailable. A connected subscription (Claude Code, Codex) takes jobs with the access it has in
//! holzi (spec 055 FR-032, R16).

use std::sync::Arc;

use crate::adapters::ProviderAdapter;
use crate::model_capabilities::ModelCapabilities;
use crate::providers::build_adapter;
use crate::storage::models::ModelRow;
use crate::storage::providers::{Provider, ProviderKind};
use crate::vault_gate::ChildRegistry;

use super::delegate::{job_context, JobAgent};
use super::place::{place_of, Place};

/// A model a job may be sent to, with what the run needs to know about it.
#[derive(Debug, Clone)]
pub(crate) struct JobModel {
    /// `models.id`.
    pub id: String,
    /// The display name, as the provenance and the model list name it.
    pub name: String,
    /// The id the provider knows the model by: `<provider>:<remote>` loses its prefix.
    pub remote_id: String,
    pub provider: Provider,
    pub place: Place,
    pub capabilities: Option<ModelCapabilities>,
}

impl JobModel {
    pub fn new(model: ModelRow, provider: Provider) -> Self {
        let remote_id = model
            .id
            .split_once(':')
            .map_or(model.id.as_str(), |(_, remote)| remote)
            .to_owned();
        Self {
            place: place_of(provider.kind, provider.base_url.as_deref()),
            id: model.id,
            name: model.name,
            remote_id,
            provider,
            capabilities: model.capabilities,
        }
    }

    /// Whether the model is an agent with tools of its own (a subscription), not a plain model: the
    /// grant says so to the user (FR-032).
    pub fn runs_agent(&self) -> bool {
        self.provider.kind == ProviderKind::CliDelegate
    }

    /// The values a message about this model must never carry: the provider's key and address.
    pub fn secrets(&self) -> Vec<String> {
        let key = self
            .provider
            .credentials
            .as_deref()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(str::to_owned);
        key.into_iter()
            .chain(self.provider.base_url.clone())
            .filter(|value| !value.is_empty())
            .collect()
    }
}

/// Builds the adapter of a model when it is tried. An error is a message for the provenance;
/// the caller sanitises it.
pub(crate) trait ModelSource: Send + Sync {
    fn adapter(&self, model: &JobModel) -> Result<Arc<dyn ProviderAdapter>, String>;
}

/// holzi's own source: remote models through their provider, a subscription through its agent.
#[derive(Default)]
pub(crate) struct Providers {
    /// Where the processes of an agent register, so that closing the vault ends them.
    children: ChildRegistry,
}

impl Providers {
    pub(crate) fn new(children: ChildRegistry) -> Self {
        Self { children }
    }
}

impl ModelSource for Providers {
    fn adapter(&self, model: &JobModel) -> Result<Arc<dyn ProviderAdapter>, String> {
        match model.provider.kind {
            ProviderKind::ApiKey => build_adapter(&model.provider, None)
                .map(Arc::from)
                .map_err(|error| error.to_string()),
            ProviderKind::Local => Err("local models cannot take jobs yet".into()),
            ProviderKind::CliDelegate => {
                build_adapter(&model.provider, Some(job_context(self.children.clone())))
                    .map(|inner| Arc::new(JobAgent::new(inner)) as Arc<dyn ProviderAdapter>)
                    .map_err(|error| error.to_string())
            }
        }
    }
}

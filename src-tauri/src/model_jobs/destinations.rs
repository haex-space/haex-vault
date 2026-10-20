//! Where a consent sends the data of a job (spec 055 US4, T048, T049): the provider, its host and
//! place as the question and the settings name them, and what one step would send. Only holzi's
//! window sees the host; the extension never does (FR-030).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::job::Step;
use super::place::place_of;
use crate::storage::providers::get_provider;
use crate::storage::query::Query;

/// The provider a consent is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionModelDestination {
    /// The display name of the provider.
    pub provider: String,
    /// The host of its address with the port, if it names one.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// `this_device`, `network` or `cloud`.
    pub place: String,
}

/// What one step sends to a destination: kinds and sizes, never the content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ExtensionModelTransfer {
    /// `text`, and `image` when the step carries images.
    pub kinds: Vec<String>,
    /// The instruction and the text input.
    #[ts(type = "number")]
    pub text_bytes: u64,
    #[ts(type = "number")]
    pub image_count: u64,
    #[ts(type = "number")]
    pub image_bytes: u64,
    /// The step also sends the result of an earlier step, whose size is known only then.
    pub earlier_result: bool,
}

impl ExtensionModelTransfer {
    pub(crate) fn of(step: &Step) -> Self {
        let images = &step.input.images;
        let mut kinds = vec!["text".to_owned()];
        if !images.is_empty() {
            kinds.push("image".to_owned());
        }
        let text_bytes = step.instruction.len() + step.input.text.as_ref().map_or(0, String::len);
        Self {
            kinds,
            text_bytes: text_bytes as u64,
            image_count: images.len() as u64,
            image_bytes: images.iter().map(|i| i.bytes.len() as u64).sum(),
            earlier_result: step.input.from.is_some(),
        }
    }
}

/// The destination a `modelDestination` target names; `None` for a provider that is gone or a
/// target that is not a provider id.
pub(crate) fn destination(
    q: &mut impl Query,
    target: &str,
) -> haex_crdt::Result<Option<ExtensionModelDestination>> {
    let Ok(id) = Uuid::parse_str(target) else {
        return Ok(None);
    };
    Ok(get_provider(q, id)?.map(|provider| {
        let address = provider.base_url.as_deref();
        ExtensionModelDestination {
            host: address.and_then(host_of),
            place: place_of(provider.kind, address).as_str().to_owned(),
            provider: provider.name,
        }
    }))
}

fn host_of(address: &str) -> Option<String> {
    let url = url::Url::parse(address).ok()?;
    let host = url.host_str()?;
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}

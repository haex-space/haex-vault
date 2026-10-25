//! Tests for the source of a job's adapter (spec 055 T022): a remote model is built from its rows
//! when it is tried, a connected subscription gets its agent, a local model one over the chat's
//! place; a provider that cannot be built and a local model without a place give no adapter.

use std::sync::Arc;

use async_trait::async_trait;

use super::local_slot::{Loan, LocalSlot};
use super::model_source::{JobModel, ModelSource, Providers};
use super::place::Place;
use super::test_support::{model_row, provider};
use crate::storage::providers::ProviderKind;

/// A slot that lends nothing: building the adapter must not ask it.
struct NoSlot;

#[async_trait]
impl LocalSlot for NoSlot {
    async fn lend(&self, _model_id: &str) -> Result<Loan, String> {
        Err("not asked".into())
    }
}

#[test]
fn a_model_names_the_id_its_provider_knows_and_where_it_runs() {
    let server = provider(
        ProviderKind::ApiKey,
        Some("http://192.168.1.20:8080/v1"),
        None,
    );
    let model = JobModel::new(model_row(&server, "qwen3-vl:8b", None), server.clone());

    assert_eq!(model.id, format!("{}:qwen3-vl:8b", server.id));
    // Only the provider's prefix goes: the remote id may hold a colon itself.
    assert_eq!(model.remote_id, "qwen3-vl:8b");
    assert_eq!(model.name, "qwen3-vl:8b");
    assert_eq!(model.place, Place::Network);

    let cloud = provider(
        ProviderKind::ApiKey,
        Some("https://api.example.com/v1"),
        None,
    );
    assert_eq!(
        JobModel::new(model_row(&cloud, "m", None), cloud).place,
        Place::Cloud
    );
}

#[test]
fn the_secrets_of_a_model_are_its_key_and_its_address() {
    let server = provider(
        ProviderKind::ApiKey,
        Some("https://ai.school.example/v1"),
        Some("sk-school-123"),
    );
    let model = JobModel::new(model_row(&server, "m", None), server);
    assert_eq!(
        model.secrets(),
        ["sk-school-123", "https://ai.school.example/v1"]
    );

    let keyless = provider(
        ProviderKind::ApiKey,
        Some("http://localhost:11434/v1"),
        None,
    );
    assert_eq!(
        JobModel::new(model_row(&keyless, "m", None), keyless).secrets(),
        ["http://localhost:11434/v1"]
    );
}

#[test]
fn a_remote_model_gets_the_adapter_of_its_provider() {
    let server = provider(
        ProviderKind::ApiKey,
        Some("http://localhost:11434/v1"),
        None,
    );
    let model = JobModel::new(model_row(&server, "m", None), server);
    assert!(Providers::default().adapter(&model).is_ok());
}

#[test]
fn a_provider_that_cannot_be_built_gives_a_message_and_no_adapter() {
    let mut legacy = provider(ProviderKind::ApiKey, Some("https://api.example.com"), None);
    legacy.adapter = None;
    let model = JobModel::new(model_row(&legacy, "m", None), legacy);
    let message = Providers::default()
        .adapter(&model)
        .err()
        .expect("no adapter");
    assert!(message.contains("missing adapter"), "{message}");
}

#[test]
fn a_local_model_gets_an_adapter_when_the_chat_has_a_place_to_lend() {
    let installed = provider(ProviderKind::Local, None, None);
    let model = JobModel::new(model_row(&installed, "m", None), installed);
    let slot: Arc<dyn LocalSlot> = Arc::new(NoSlot);

    assert!(Providers::new(Default::default(), Some(slot))
        .adapter(&model)
        .is_ok());
    assert!(Providers::default().adapter(&model).is_err());
}

#[test]
fn a_connected_subscription_gets_an_adapter_and_a_broken_one_gives_a_message() {
    let mut agent = provider(ProviderKind::CliDelegate, Some("claude"), Some("token"));
    agent.adapter = Some("claude".into());
    let model = JobModel::new(model_row(&agent, "m", None), agent.clone());
    assert!(model.runs_agent());
    assert!(Providers::default().adapter(&model).is_ok());

    // An adapter nobody knows: the job tries the next model.
    agent.adapter = Some("unknown".into());
    let model = JobModel::new(model_row(&agent, "m", None), agent);
    let message = Providers::default()
        .adapter(&model)
        .err()
        .expect("no adapter");
    assert!(message.contains("invalid or missing adapter"), "{message}");
}

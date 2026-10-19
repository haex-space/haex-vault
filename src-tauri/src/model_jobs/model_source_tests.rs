//! Tests for the source of a job's adapter (spec 055 T022): a remote model is built from its rows
//! when it is tried; a provider that cannot be built, a local model and a command-line agent give
//! no adapter.

use super::model_source::{JobModel, ModelSource, Providers};
use super::place::Place;
use super::test_support::{model_row, provider};
use crate::storage::providers::ProviderKind;

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
    assert!(Providers.adapter(&model).is_ok());
}

#[test]
fn a_provider_that_cannot_be_built_gives_a_message_and_no_adapter() {
    let mut legacy = provider(ProviderKind::ApiKey, Some("https://api.example.com"), None);
    legacy.adapter = None;
    let model = JobModel::new(model_row(&legacy, "m", None), legacy);
    let message = Providers.adapter(&model).err().expect("no adapter");
    assert!(message.contains("missing adapter"), "{message}");
}

#[test]
fn a_local_model_and_a_command_line_agent_take_no_job() {
    for kind in [ProviderKind::Local, ProviderKind::CliDelegate] {
        let source = provider(kind, None, None);
        let model = JobModel::new(model_row(&source, "m", None), source);
        assert!(Providers.adapter(&model).is_err(), "{kind:?}");
    }
}

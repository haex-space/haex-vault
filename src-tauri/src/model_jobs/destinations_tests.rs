//! Tests for what a consent names (spec 055 US4, T048, T049): the provider with its host and place,
//! and what a step would send.

use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use super::destinations::{destination, ExtensionModelDestination, ExtensionModelTransfer};
use super::job::parse;
use super::test_support::{provider, store_models};
use crate::passwords::test_support::open_test_vault;
use crate::storage::providers::ProviderKind;
use crate::vault_gate::{VaultDb, VaultGate};

fn steps() -> serde_json::Value {
    json!({ "steps": [
        {
            "id": "read",
            "needs": ["text", "image"],
            "instruction": "Lies die Seite.",
            "input": { "text": "Seite 3", "images": [
                { "mimeType": "image/png", "data": "iVBORw0KGgo=" },
                { "mimeType": "image/png", "data": "iVBORw0KGgo=" }
            ] },
            "format": { "name": "page", "version": 1, "schema": { "type": "object" } }
        },
        {
            "id": "fix",
            "needs": ["text"],
            "instruction": "Korrigiere nur Rechtschreibung.",
            "input": { "from": { "step": "read", "pointer": "" } },
            "format": { "name": "text-fix", "version": 1, "schema": { "type": "object" } }
        }
    ]})
}

#[test]
fn a_transfer_names_kinds_and_sizes_and_an_earlier_result_without_its_size() {
    let job = parse(&steps()).unwrap();

    assert_eq!(
        ExtensionModelTransfer::of(&job.steps[0]),
        ExtensionModelTransfer {
            kinds: vec!["text".into(), "image".into()],
            text_bytes: ("Lies die Seite.".len() + "Seite 3".len()) as u64,
            image_count: 2,
            image_bytes: 16,
            earlier_result: false,
        }
    );
    assert_eq!(
        ExtensionModelTransfer::of(&job.steps[1]),
        ExtensionModelTransfer {
            kinds: vec!["text".into()],
            text_bytes: "Korrigiere nur Rechtschreibung.".len() as u64,
            image_count: 0,
            image_bytes: 0,
            earlier_result: true,
        }
    );
}

fn vault() -> (tempfile::TempDir, VaultDb) {
    let (dir, db) = open_test_vault();
    (dir, VaultGate::new().vault_db(Arc::new(db)).unwrap())
}

fn destination_of(db: &VaultDb, target: &str) -> Option<ExtensionModelDestination> {
    let target = target.to_owned();
    db.read_blocking(move |q| destination(q, &target)).unwrap()
}

#[test]
fn a_destination_names_the_provider_its_host_with_the_port_and_its_place() {
    let (_dir, db) = vault();
    for (address, host, place) in [
        (
            Some("http://192.168.1.20:11434/v1"),
            Some("192.168.1.20:11434"),
            "network",
        ),
        (
            Some("https://api.example.com/v1"),
            Some("api.example.com"),
            "cloud",
        ),
        (
            Some("http://[fd00::5]:8080/v1"),
            Some("[fd00::5]:8080"),
            "network",
        ),
        (Some("not an address"), None, "cloud"),
    ] {
        let server = provider(ProviderKind::ApiKey, address, Some("sk-test"));
        store_models(&db, &server, &[]);
        assert_eq!(
            destination_of(&db, &server.id.to_string()),
            Some(ExtensionModelDestination {
                provider: "Test provider".into(),
                host: host.map(str::to_owned),
                place: place.into(),
            }),
            "{address:?}"
        );
    }
}

#[test]
fn a_provider_that_is_gone_or_a_target_that_is_none_names_nothing() {
    let (_dir, db) = vault();
    let gone = Uuid::from_u128(0x123456789abcdef0123456789abcdef0);
    assert_eq!(destination_of(&db, &gone.to_string()), None);
    assert_eq!(destination_of(&db, "not-a-provider"), None);
}

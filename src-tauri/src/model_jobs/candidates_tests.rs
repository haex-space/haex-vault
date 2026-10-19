//! Tests for the candidates of a step (spec 055 T024): the models the extension may use, with the
//! highest effort allowed, that fit what the step needs, in the user's rank and then in the order
//! of the grants. Reachability is never looked at here.

use std::path::Path;
use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use super::candidates::{ceiling, for_step, granted, job_models, model_permissions, Candidate};
use super::effort::Level;
use super::job;
use super::test_support::{model_row, provider, remember, store_models};
use crate::adapters::AttachmentKind;
use crate::extensions::permissions::order::set_order;
use crate::extensions::permissions::{
    Action, GrantScope, Permission, PermissionKind, PermissionStatus, Target,
};
use crate::extensions::registry::install::install;
use crate::model_capabilities::ModelCapabilities;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::storage::providers::{ProviderCapability, ProviderKind};
use crate::vault_gate::{VaultDb, VaultGate};

struct Vault {
    _dir: tempfile::TempDir,
    db: VaultDb,
    extension: Uuid,
    device: Uuid,
}

fn vault() -> Vault {
    let (dir, db) = open_test_vault();
    let db = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = db
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-notes-like.xt"),
    )
    .unwrap();
    let extension = install(&db, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    Vault {
        _dir: dir,
        db,
        extension,
        device,
    }
}

fn images() -> Option<ModelCapabilities> {
    Some(ModelCapabilities {
        accepted_attachment_kinds: Some(vec![AttachmentKind::Image]),
        ..Default::default()
    })
}

impl Vault {
    fn grant(&self, model: &str, effort: &str, at_ms: i64) {
        remember(
            &self.db,
            self.extension,
            ("model", effort, model),
            "granted",
            at_ms,
        );
    }

    fn candidates(&self, held: Vec<Permission>) -> Vec<Candidate> {
        let (extension, device) = (self.extension, self.device);
        self.db
            .read_blocking(move |q| {
                let permissions = model_permissions(q, extension, device, held)?;
                granted(q, extension, device, &permissions).map_err(Into::into)
            })
            .unwrap()
    }

    fn ids(&self, held: Vec<Permission>) -> Vec<(String, Level)> {
        self.candidates(held)
            .into_iter()
            .map(|c| (c.model.name, c.ceiling))
            .collect()
    }
}

#[test]
fn only_granted_models_are_candidates_with_the_highest_effort_allowed() {
    let v = vault();
    let server = provider(ProviderKind::ApiKey, Some("http://localhost:8080/v1"), None);
    let rows = [
        model_row(&server, "a", None),
        model_row(&server, "b", None),
        model_row(&server, "c", None),
    ];
    store_models(&v.db, &server, &rows);
    v.grant(&rows[0].id, "low", 1);
    v.grant(&rows[0].id, "high", 1);
    v.grant(&rows[1].id, "none", 2);

    assert_eq!(
        v.ids(vec![]),
        [("a".to_owned(), Level::High), ("b".to_owned(), Level::None)]
    );
}

#[test]
fn a_denial_of_a_model_takes_it_away_whatever_its_effort() {
    let v = vault();
    let server = provider(ProviderKind::ApiKey, Some("http://localhost:8080/v1"), None);
    let rows = [model_row(&server, "a", None), model_row(&server, "b", None)];
    store_models(&v.db, &server, &rows);
    v.grant("*", "high", 1);
    // A denial of `medium` alone would leave `high` allowed and a wish of `low` denied.
    remember(
        &v.db,
        v.extension,
        ("model", "medium", &rows[0].id),
        "denied",
        2,
    );

    assert_eq!(v.ids(vec![]), [("b".to_owned(), Level::High)]);
}

#[test]
fn every_model_counts_for_a_grant_of_all_but_no_agent_no_transcription_and_no_orphan() {
    let v = vault();
    let server = provider(ProviderKind::ApiKey, Some("http://localhost:8080/v1"), None);
    let agent = provider(
        ProviderKind::CliDelegate,
        Some("/usr/bin/claude"),
        Some("x"),
    );
    let mut whisper = provider(ProviderKind::Local, None, None);
    whisper.capability = ProviderCapability::Transcription;
    store_models(&v.db, &server, &[model_row(&server, "a", None)]);
    store_models(&v.db, &agent, &[model_row(&agent, "claude", None)]);
    store_models(&v.db, &whisper, &[model_row(&whisper, "whisper", None)]);
    // A model whose provider is gone (a sync that has not caught up).
    let gone = provider(ProviderKind::ApiKey, Some("http://localhost:1/v1"), None);
    let orphan = model_row(&gone, "orphan", None);
    v.db.write_blocking(move |tx| crate::storage::models::upsert_model(tx, &orphan).map(drop))
        .unwrap();
    v.grant("*", "medium", 1);

    assert_eq!(v.ids(vec![]), [("a".to_owned(), Level::Medium)]);
    let all =
        v.db.read_blocking(|q| job_models(q).map_err(Into::into))
            .unwrap();
    assert_eq!(all.len(), 1);
}

#[test]
fn the_rank_comes_first_then_the_time_of_the_grant_then_a_grant_for_the_session() {
    let v = vault();
    let server = provider(ProviderKind::ApiKey, Some("http://localhost:8080/v1"), None);
    let rows: Vec<_> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|name| model_row(&server, name, None))
        .collect();
    store_models(&v.db, &server, &rows);
    v.grant(&rows[0].id, "low", 30);
    v.grant(&rows[1].id, "low", 10);
    v.grant(&rows[2].id, "low", 20);
    v.grant(&rows[3].id, "low", 5);
    let ranked = vec![rows[2].id.clone(), rows[0].id.clone()];
    let extension = v.extension;
    v.db.write_blocking(move |tx| set_order(tx, extension, &ranked).map_err(Into::into))
        .unwrap();
    let held = Permission {
        kind: PermissionKind::Model,
        action: Action::Effort(Level::Low),
        target: Target::Model(rows[4].id.clone()),
        status: PermissionStatus::Granted,
        scope: GrantScope::Vault,
    };

    let names: Vec<String> = v.ids(vec![held]).into_iter().map(|(n, _)| n).collect();

    // c and a by rank; then d (5) and b (10) by grant time; e is held for the session only.
    assert_eq!(names, ["c", "a", "d", "b", "e"]);
}

#[test]
fn a_step_with_images_goes_only_to_models_that_take_images() {
    let v = vault();
    let server = provider(ProviderKind::ApiKey, Some("http://localhost:8080/v1"), None);
    let texts_only = Some(ModelCapabilities {
        accepted_attachment_kinds: Some(vec![]),
        ..Default::default()
    });
    let rows = [
        model_row(&server, "vision", images()),
        model_row(&server, "text", texts_only),
        model_row(&server, "unknown", None),
    ];
    store_models(&v.db, &server, &rows);
    v.grant("*", "low", 1);
    let all = v.candidates(vec![]);
    let step = |value| job::parse(&value).unwrap().steps.remove(0);
    let format = json!({ "name": "f", "version": 1, "schema": { "type": "object" } });
    let read = step(json!({ "steps": [{
        "id": "read", "needs": ["text", "image"], "instruction": "Lies.",
        "input": { "images": [{ "mimeType": "image/png", "data": "iVBORw0KGgo=" }] },
        "format": format,
    }]}));
    let fix = step(json!({ "steps": [{
        "id": "fix", "needs": ["text"], "instruction": "Korrigiere.",
        "input": { "text": "Tsest" }, "format": format,
    }]}));

    let names =
        |list: Vec<Candidate>| -> Vec<String> { list.into_iter().map(|c| c.model.name).collect() };
    // A model nobody knows about takes no image (FR-019).
    assert_eq!(names(for_step(&all, &read)), ["vision"]);
    assert_eq!(names(for_step(&all, &fix)), ["text", "unknown", "vision"]);
}

#[test]
fn a_ceiling_needs_every_effort_below_it() {
    let model = "6f1c1d52-3a8b-4f0e-9c55-0a1b2c3d4e5f:m";
    let row = |action, status| Permission {
        kind: PermissionKind::Model,
        action: Action::Effort(action),
        target: Target::Model(model.to_owned()),
        status,
        scope: GrantScope::Vault,
    };
    let device = Uuid::new_v4();
    assert_eq!(ceiling(&[], model, device), None);
    assert_eq!(
        ceiling(
            &[row(Level::Medium, PermissionStatus::Granted)],
            model,
            device
        ),
        Some(Level::Medium)
    );
    assert_eq!(
        ceiling(
            &[
                row(Level::High, PermissionStatus::Granted),
                row(Level::Low, PermissionStatus::Denied)
            ],
            model,
            device
        ),
        None
    );
    assert_eq!(
        ceiling(&[row(Level::High, PermissionStatus::Ask)], model, device),
        None
    );
}

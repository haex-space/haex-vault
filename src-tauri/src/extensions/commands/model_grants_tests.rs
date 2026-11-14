//! Tests for the models of an extension in its settings and in the question of a job (spec 055
//! T031, T033): list, grant with a ceiling, revoke and choose the default, each holding from the next start;
//! a start without a grant asks with the models that fit, and the answer keeps the model and the
//! ceiling the user picked.

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::*;
use crate::adapters::AttachmentKind;
use crate::extensions::bridge::dispatch::{call, CallContext, Emit};
use crate::extensions::bridge::events::FRAME_EVENT;
use crate::extensions::commands::permissions::{resolve, PermissionResolveArgs};
use crate::extensions::permissions::default_model;
use crate::extensions::permissions::prompts::{PermissionDecision, Question};
use crate::extensions::permissions::store::rows_of;
use crate::extensions::permissions::{PermissionKind, PermissionStatus};
use crate::extensions::registry::effective::effective_bundle;
use crate::extensions::registry::install::install;
use crate::model_capabilities::ModelCapabilities;
use crate::model_jobs::test_support::{model_row, provider, remember, store_models};
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::storage::models::ModelRow;
use crate::storage::providers::ProviderKind;
use crate::vault_gate::VaultGate;

#[derive(Default)]
struct Recorded(Mutex<Vec<(String, Value)>>);

impl Emit for Recorded {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorded {
    fn named(&self, name: &str) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(e, _)| e == name)
            .map(|(_, p)| p.clone())
            .collect()
    }
}

struct Setup {
    _dir: tempfile::TempDir,
    ctx: CallContext,
    recorded: Arc<Recorded>,
    extension: Uuid,
    /// `vision` (takes images), `text` and `other`, in that order.
    models: Vec<ModelRow>,
}

fn setup() -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-notes-like.xt"),
    )
    .unwrap();
    let extension = install(&vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    let bundle_id = vault
        .read_blocking(move |q| effective_bundle(q, extension).map_err(Into::into))
        .unwrap()
        .unwrap()
        .bundle_id;
    let server = provider(
        ProviderKind::ApiKey,
        Some("http://localhost:11434/v1"),
        None,
    );
    let capabilities = |images: bool| {
        Some(ModelCapabilities {
            accepted_attachment_kinds: Some(if images {
                vec![AttachmentKind::Image]
            } else {
                vec![]
            }),
            ..Default::default()
        })
    };
    let models = vec![
        model_row(&server, "vision", capabilities(true)),
        model_row(&server, "text", capabilities(false)),
        model_row(&server, "other", capabilities(false)),
    ];
    store_models(&vault, &server, &models);
    let host = Arc::new(ExtensionHost::default());
    let session = host.frames.open(extension, bundle_id, "tab");
    let recorded = Arc::new(Recorded::default());
    Setup {
        _dir: dir,
        ctx: CallContext {
            db: vault,
            host,
            session,
            device,
            emitter: Arc::clone(&recorded) as Arc<dyn Emit>,
        },
        recorded,
        extension,
        models,
    }
}

impl Setup {
    fn id(&self, index: usize) -> String {
        self.models[index].id.clone()
    }

    fn grant(&self, index: usize, effort: &str) {
        set(
            &self.ctx.db,
            &self.ctx.host,
            ExtensionModelGrantArgs {
                extension_id: self.extension.to_string(),
                model_id: self.id(index),
                effort: effort.into(),
            },
            1,
        )
        .unwrap();
    }

    fn entries(&self) -> Vec<ExtensionModelEntry> {
        list(
            &self.ctx.db,
            &self.ctx.host,
            self.ctx.device,
            self.extension,
        )
        .unwrap()
    }

    /// What the next start sees: the models the extension may use, first choice first.
    fn usable(&self) -> Vec<(String, String)> {
        let answer = call(&self.ctx, "extension_model_list", &json!({})).unwrap();
        answer["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["name"].as_str().unwrap().to_owned(),
                    m["maxEffort"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }

    fn model_rows(&self) -> Vec<(String, String, String)> {
        let extension = self.extension;
        self.ctx
            .db
            .read_blocking(move |q| rows_of(q, extension).map_err(Into::into))
            .unwrap()
            .into_iter()
            .filter(|r| r.kind == "model")
            .map(|r| (r.action, r.target, r.status))
            .collect()
    }

    fn start(&self, step: Value) -> crate::extensions::error::BridgeError {
        call(
            &self.ctx,
            "extension_model_job_start",
            &json!({ "job": { "steps": [step] } }),
        )
        .unwrap_err()
    }

    fn answer(&self, decision: PermissionDecision, remember: bool, pick: Option<(String, &str)>) {
        let request = self
            .recorded
            .named("extension-permission-request")
            .pop()
            .unwrap();
        resolve(
            &self.ctx.db,
            &self.ctx.host,
            &*self.recorded,
            self.ctx.device,
            PermissionResolveArgs {
                request_id: request["requestId"].as_str().unwrap().to_owned(),
                decision,
                remember,
                model: pick.map(|(model_id, effort)| ExtensionModelPick {
                    model_id,
                    effort: effort.into(),
                    make_default: false,
                }),
            },
            2,
        )
        .unwrap();
    }
}

fn text_step() -> Value {
    json!({
        "id": "fix", "needs": ["text"], "instruction": "Korrigiere.",
        "input": { "text": "Tsest" },
        "format": { "name": "text-fix", "version": 1, "schema": { "type": "object" } }
    })
}

fn image_step() -> Value {
    let mut step = text_step();
    step["needs"] = json!(["text", "image"]);
    step["input"]["images"] = json!([{ "mimeType": "image/png", "data": "iVBORw0KGgo=" }]);
    step
}

#[test]
fn the_list_shows_the_usable_models_first_with_their_ceiling_then_the_others() {
    let s = setup();
    s.grant(1, "medium");
    remember(
        &s.ctx.db,
        s.extension,
        ("model", "low", &s.id(2)),
        "denied",
        1,
    );

    let entries = s.entries();

    let summary: Vec<(&str, Option<&str>, bool)> = entries
        .iter()
        .map(|e| (e.name.as_str(), e.ceiling.as_deref(), e.denied))
        .collect();
    assert_eq!(
        summary,
        [
            ("text", Some("medium"), false),
            ("other", None, true),
            ("vision", None, false)
        ]
    );
    assert_eq!(entries[0].place, "this_device");
    assert_eq!(entries[0].provider, "Test provider");
    assert!(entries[2].takes_images && !entries[0].takes_images);
    assert!(!entries[0].held);
}

#[test]
fn a_grant_replaces_every_earlier_row_of_the_model() {
    let s = setup();
    remember(
        &s.ctx.db,
        s.extension,
        ("model", "high", &s.id(0)),
        "denied",
        1,
    );
    s.grant(1, "high");
    s.grant(0, "low");
    s.grant(1, "none");

    // One row per model: the denial and the earlier ceiling are gone.
    let mut rows = s.model_rows();
    rows.sort();
    assert_eq!(
        rows,
        [
            ("low".into(), s.id(0), "granted".into()),
            ("none".into(), s.id(1), "granted".into())
        ]
    );
    // No default: the models come in the order they were granted.
    assert_eq!(
        s.usable(),
        [
            ("text".to_owned(), "none".to_owned()),
            ("vision".to_owned(), "low".to_owned())
        ]
    );
}

#[test]
fn stored_model_updates_forget_conflicting_session_decisions() {
    let s = setup();
    let held = |model_id: String, action: &str| Question {
        extension_id: s.extension,
        kind: PermissionKind::Model,
        action: action.into(),
        target: model_id,
    };
    let granted = held(s.id(0), "high");
    s.ctx
        .host
        .permissions
        .hold(&granted, PermissionStatus::Granted);
    let denied = held(s.id(1), "medium");
    s.ctx
        .host
        .permissions
        .hold(&denied, PermissionStatus::Denied);
    s.grant(0, "low");
    s.grant(1, "none");
    assert!(s
        .ctx
        .host
        .permissions
        .temporary(s.extension, PermissionKind::Model)
        .is_empty());
    // Granted at the same time: by model id.
    assert_eq!(
        s.usable(),
        [
            ("text".to_owned(), "none".to_owned()),
            ("vision".to_owned(), "low".to_owned())
        ]
    );
}

fn default_of(s: &Setup) -> Option<String> {
    let extension = s.extension;
    s.ctx
        .db
        .read_blocking(move |q| default_model::get(q, extension).map_err(Into::into))
        .unwrap()
}

fn set_default(s: &Setup, model: Option<String>) -> Result<()> {
    super::set_default(
        &s.ctx.db,
        &s.ctx.host,
        s.ctx.device,
        ExtensionModelDefaultArgs {
            extension_id: s.extension.to_string(),
            model_id: model,
        },
    )
}

#[test]
fn the_default_model_comes_first_and_revoking_it_takes_the_default_away() {
    let s = setup();
    s.grant(0, "low");
    s.grant(1, "low");
    s.grant(2, "low");
    set_default(&s, Some(s.id(2))).unwrap();
    let names = |s: &Setup| -> Vec<String> { s.usable().into_iter().map(|(n, _)| n).collect() };
    assert_eq!(names(&s), ["other", "text", "vision"]);
    assert!(s.entries().iter().filter(|e| e.is_default).count() == 1);
    assert!(s
        .entries()
        .iter()
        .any(|e| e.is_default && e.name == "other"));

    revoke(
        &s.ctx.db,
        &s.ctx.host,
        ExtensionModelRevokeArgs {
            extension_id: s.extension.to_string(),
            model_id: s.id(2),
        },
    )
    .unwrap();

    assert_eq!(names(&s), ["text", "vision"]);
    assert_eq!(
        default_of(&s),
        None,
        "a model that is revoked is no default"
    );
    assert!(s
        .model_rows()
        .iter()
        .all(|(_, target, _)| *target != s.id(2)));
}

#[test]
fn a_new_default_replaces_the_old_one_and_a_new_grant_of_it_keeps_it() {
    let s = setup();
    s.grant(0, "low");
    s.grant(1, "low");
    set_default(&s, Some(s.id(0))).unwrap();
    set_default(&s, Some(s.id(1))).unwrap();
    assert_eq!(default_of(&s), Some(s.id(1)));

    // The user raises the effort of the default: it stays the default.
    s.grant(1, "high");
    assert_eq!(default_of(&s), Some(s.id(1)));

    // A denial takes the model away from the default.
    let (extension, denied) = (s.extension, s.id(1));
    s.ctx
        .db
        .write_blocking(move |tx| {
            grants::put(
                tx,
                extension,
                &denied,
                PermissionStatus::Denied,
                Level::None,
                3,
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(default_of(&s), None);
}

#[test]
fn only_a_model_the_extension_may_use_can_become_the_default() {
    let s = setup();
    s.grant(0, "low");
    set_default(&s, Some(s.id(0))).unwrap();

    // Never granted, and revoked after the list was loaded: both are refused and nothing changes.
    assert!(matches!(
        set_default(&s, Some(s.id(1))),
        Err(HolziError::InvalidInput { .. })
    ));
    revoke(
        &s.ctx.db,
        &s.ctx.host,
        ExtensionModelRevokeArgs {
            extension_id: s.extension.to_string(),
            model_id: s.id(0),
        },
    )
    .unwrap();
    assert!(matches!(
        set_default(&s, Some(s.id(0))),
        Err(HolziError::InvalidInput { .. })
    ));
    assert_eq!(default_of(&s), None, "the revocation took the default away");
    set_default(&s, None).unwrap();
}

#[test]
fn answering_a_question_can_make_the_picked_model_the_default() {
    let s = setup();
    s.start(image_step());
    let request = s
        .recorded
        .named("extension-permission-request")
        .pop()
        .unwrap();
    assert!(
        request.get("defaultModel").is_none(),
        "the extension has no default to name"
    );
    resolve(
        &s.ctx.db,
        &s.ctx.host,
        &*s.recorded,
        s.ctx.device,
        PermissionResolveArgs {
            request_id: request["requestId"].as_str().unwrap().to_owned(),
            decision: PermissionDecision::Allow,
            remember: true,
            model: Some(ExtensionModelPick {
                model_id: s.id(0),
                effort: "none".into(),
                make_default: true,
            }),
        },
        2,
    )
    .unwrap();
    assert_eq!(default_of(&s), Some(s.id(0)));
}

#[test]
fn the_question_names_the_default_that_cannot_take_the_job() {
    let s = setup();
    s.grant(1, "low");
    set_default(&s, Some(s.id(1))).unwrap();

    // `text` cannot read an image and no granted model can: a request for access asks for a grant
    // and names the default. (A job asks through the replacement question instead, which never
    // picks a model on its own: `model_jobs_ask_tests.rs`.)
    call(
        &s.ctx,
        "extension_model_access_request",
        &json!({ "needs": ["text", "image"] }),
    )
    .unwrap_err();
    let request = s
        .recorded
        .named("extension-permission-request")
        .pop()
        .unwrap();
    assert_eq!(request["defaultModel"], "text");
}

#[test]
fn a_wrong_effort_or_model_id_is_refused() {
    let s = setup();
    for (model_id, effort) in [
        (s.id(0), "bogus"),
        ("*".to_owned(), "low"),
        (String::new(), "low"),
    ] {
        let refused = set(
            &s.ctx.db,
            &s.ctx.host,
            ExtensionModelGrantArgs {
                extension_id: s.extension.to_string(),
                model_id,
                effort: effort.into(),
            },
            1,
        );
        assert!(
            matches!(refused, Err(HolziError::InvalidInput { .. })),
            "{effort}"
        );
    }
    assert!(s.model_rows().is_empty());
}

#[test]
fn a_start_without_a_grant_asks_with_the_models_that_fit() {
    let s = setup();

    let refused = s.start(image_step());

    assert_eq!(refused.code.as_u16(), 1004);
    assert_eq!(
        refused.details,
        Some(json!({
            "resourceType": "model", "action": "none", "target": s.id(0),
            "needs": ["text", "image"]
        }))
    );
    let question = s
        .recorded
        .named("extension-permission-request")
        .pop()
        .unwrap();
    assert_eq!(question["kind"], "model");
    let offered: Vec<&str> = question["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(offered, ["vision"], "only models that take images");

    // A text step offers every model, the proposed one first.
    s.start(text_step());
    let question = s
        .recorded
        .named("extension-permission-request")
        .pop()
        .unwrap();
    let offered: Vec<&str> = question["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        offered[0],
        question["target"]
            .as_str()
            .and_then(|id| s.models.iter().find(|m| m.id == id))
            .map(|m| m.name.as_str())
            .unwrap()
    );
    assert_eq!(offered.len(), 3);
}

#[test]
fn the_answer_keeps_the_picked_model_and_ceiling_remembered_or_for_the_session() {
    let s = setup();
    s.start(text_step());

    // The user picks another model than proposed and lowers the ceiling.
    s.answer(PermissionDecision::Allow, true, Some((s.id(2), "low")));

    assert_eq!(s.model_rows(), [("low".into(), s.id(2), "granted".into())]);
    assert_eq!(s.usable(), [("other".to_owned(), "low".to_owned())]);
    let resolved: Vec<Value> = s
        .recorded
        .named(FRAME_EVENT)
        .into_iter()
        .filter(|e| e["type"] == "extension:permission-resolved")
        .collect();
    // The frame hears the decision for the model its 1004 named and repeats its call.
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0]["data"]["decision"], "granted");

    // For the session only: nothing is written, the list says so.
    let fresh = setup();
    fresh.start(text_step());
    fresh.answer(
        PermissionDecision::Allow,
        false,
        Some((fresh.id(1), "high")),
    );
    assert!(fresh.model_rows().is_empty());
    let entry = fresh.entries().into_iter().next().unwrap();
    assert_eq!((entry.name.as_str(), entry.held), ("text", true));
    assert_eq!(entry.ceiling.as_deref(), Some("high"));
}

#[test]
fn a_denial_keeps_the_model_away_and_the_next_start_asks_for_another() {
    let s = setup();
    s.start(image_step());
    s.answer(PermissionDecision::Deny, true, None);

    assert_eq!(s.model_rows(), [("none".into(), s.id(0), "denied".into())]);
    // The only model that takes images is refused.
    let refused = s.start(image_step());
    assert_eq!(refused.code.as_u16(), 1002);

    // A text step still asks, for a model the user has not refused.
    let refused = s.start(text_step());
    assert_eq!(refused.code.as_u16(), 1004);
    assert_ne!(refused.details.unwrap()["target"], json!(s.id(0)));
}

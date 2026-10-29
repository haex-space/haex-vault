//! Agents managing sync rules (spec 061 US5, scenarios 1–5, FR-035, FR-036): nothing without
//! "Sync-Regeln verwalten"; reading runs without a question; creating, changing and deleting wait
//! for the user every time, and without a yes nothing changes; holzi's own places and contacts'
//! shares are refused before anyone is asked.

// These tests insert devices directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use haex_crdt::rusqlite::params;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::{SyncRulesAgent, SyncRulesEnv};
use crate::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor};
use crate::chat::tools::permission::PermissionMode;
use crate::file_sync::agent_confirm::{
    SyncRuleConfirm, SyncRuleConfirmAction, SyncRuleConfirmRequest,
};
use crate::file_sync::commands;
use crate::file_sync::model::{
    SyncRuleDeleteMode, SyncRuleDirection, SyncRuleDraft, SyncRulePlace,
};
use crate::file_sync::rules::SyncRuleError;
use crate::files::local::OwnPlaces;
use crate::files::permissions::{self, AgentFileKind, AgentFilePermission, AgentFileStatus};
use crate::files::SourceRef;
use crate::remote_storage::test_support::vault;
use crate::vault_gate::VaultDb;

const DEVICE: &str = "11111111-1111-4111-8111-111111111111";
const OWN: &str = "/data/holzi";

struct FakeEnv {
    db: VaultDb,
    confirm: SyncRuleConfirm,
    /// What the session heard: `stop:<id>` and `reload:<id or ->`.
    session: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl SyncRulesEnv for FakeEnv {
    async fn vault(&self) -> Result<(VaultDb, String), SyncRuleError> {
        Ok((self.db.clone(), DEVICE.to_owned()))
    }

    fn own(&self) -> OwnPlaces {
        OwnPlaces::new(vec![PathBuf::from(OWN)])
    }

    fn confirm(&self) -> &SyncRuleConfirm {
        &self.confirm
    }

    async fn stop(&self, id: &str) {
        self.session
            .lock()
            .expect("session")
            .push(format!("stop:{id}"));
    }

    fn reload(&self, stopped: Option<String>) {
        self.session
            .lock()
            .expect("session")
            .push(format!("reload:{}", stopped.as_deref().unwrap_or("-")));
    }
}

/// A vault with this device, an agent over it whose window answers every confirmation with
/// `answer`, and the confirmations it saw.
struct Scene {
    _dir: tempfile::TempDir,
    db: VaultDb,
    agent: SyncRulesAgent<FakeEnv>,
    answer: Arc<AtomicBool>,
    asked: Arc<Mutex<Vec<SyncRuleConfirmRequest>>>,
    session: Arc<Mutex<Vec<String>>>,
}

impl Scene {
    fn new() -> Self {
        Self::with_confirm(SyncRuleConfirm::with_timeout(Duration::from_secs(5)), true)
    }

    /// `answering`: whether the window answers at all.
    fn with_confirm(confirm: SyncRuleConfirm, answering: bool) -> Self {
        let (dir, db) = vault();
        db.write_blocking(|tx| {
            tx.execute(
                "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
                 VALUES ('inst', ?1, NULL, 0)",
                params![DEVICE],
            )?;
            Ok(())
        })
        .expect("device");
        let answer = Arc::new(AtomicBool::new(true));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (seen, given, window) = (Arc::clone(&asked), Arc::clone(&answer), confirm.clone());
        confirm.set_emitter(Arc::new(move |request: &SyncRuleConfirmRequest| {
            seen.lock().expect("asked").push(request.clone());
            if answering {
                window.answer(&request.request_id, given.load(Ordering::SeqCst));
            }
            true
        }));
        let session = Arc::new(Mutex::new(Vec::new()));
        let agent = SyncRulesAgent::builtin(FakeEnv {
            db: db.clone(),
            confirm,
            session: Arc::clone(&session),
        });
        Self {
            _dir: dir,
            db,
            agent,
            answer,
            asked,
            session,
        }
    }

    fn permit(&self, status: AgentFileStatus) {
        let row = AgentFilePermission {
            agent_id: permissions::BUILTIN_AGENT.to_owned(),
            kind: AgentFileKind::SyncRules,
            target: String::new(),
            status,
            updated_at: 1,
            reach: None,
        };
        self.db
            .write_blocking(move |tx| Ok(permissions::set(tx, &row)?))
            .expect("permission");
    }

    fn answer(&self, yes: bool) {
        self.answer.store(yes, Ordering::SeqCst);
    }

    async fn run_in(
        &self,
        mode: PermissionMode,
        action: &str,
        input: Value,
        cancel: &CancellationToken,
    ) -> Result<Value, NativeError> {
        self.agent
            .run(action, input, &NativeCall::builtin(mode), cancel)
            .await
            .map(|reply| reply.result)
    }

    async fn run(&self, action: &str, input: Value) -> Result<Value, NativeError> {
        self.run_in(
            PermissionMode::Manual,
            action,
            input,
            &CancellationToken::new(),
        )
        .await
    }

    /// A rule stored by the user, without the agent.
    async fn stored(&self, a: &str, b: &str) -> String {
        commands::create(&self.db, DEVICE, &OwnPlaces::new(vec![]), draft(a, b))
            .await
            .expect("stored")
            .id
    }

    async fn rules(&self) -> Vec<String> {
        commands::list(&self.db, DEVICE, None)
            .await
            .expect("list")
            .into_iter()
            .map(|view| view.rule.name)
            .collect()
    }

    fn asked(&self) -> Vec<SyncRuleConfirmRequest> {
        self.asked.lock().expect("asked").clone()
    }

    fn session(&self) -> Vec<String> {
        self.session.lock().expect("session").clone()
    }
}

fn draft(a: &str, b: &str) -> SyncRuleDraft {
    let place = |path: &str| SyncRulePlace {
        source: SourceRef::Device,
        path: path.to_owned(),
    };
    SyncRuleDraft {
        name: "Fotos".to_owned(),
        place_a: place(a),
        place_b: place(b),
        direction: SyncRuleDirection::AToB,
        delete_mode: SyncRuleDeleteMode::Keep,
        interval_secs: 900,
        enabled: true,
        allow_decrypted: false,
    }
}

fn json_of(draft: &SyncRuleDraft) -> Value {
    serde_json::to_value(draft).expect("json")
}

#[tokio::test(flavor = "multi_thread")]
async fn without_the_permission_every_action_is_refused() {
    let scene = Scene::new();
    let id = scene.stored("/home/a", "/home/b").await;
    for status in [None, Some(AgentFileStatus::Denied)] {
        if let Some(status) = status {
            scene.permit(status);
        }
        for (action, input) in [
            ("syncRules.list", json!({})),
            ("syncRules.status", json!({ "id": id })),
            ("syncRules.create", json_of(&draft("/home/c", "/home/d"))),
            (
                "syncRules.update",
                json!({ "id": id, "draft": json_of(&draft("/home/a", "/home/z")) }),
            ),
            ("syncRules.delete", json!({ "id": id })),
        ] {
            let refused = scene.run(action, input).await.expect_err(action);
            assert_eq!(refused.code, "sync_rules_not_granted", "{action}");
            assert!(
                refused.message.contains("Sync-Regeln verwalten"),
                "names the permission (scenario 1)"
            );
        }
    }
    assert!(scene.asked().is_empty());
    assert_eq!(scene.rules().await, ["Fotos"], "nothing changed");
}

#[tokio::test(flavor = "multi_thread")]
async fn rules_and_their_status_are_read_without_a_question() {
    let scene = Scene::new();
    let id = scene.stored("/home/a", "/home/b").await;
    scene.permit(AgentFileStatus::Granted);
    let list = scene.run("syncRules.list", json!({})).await.expect("list");
    assert_eq!(list[0]["rule"]["id"], id.as_str());
    assert_eq!(list[0]["status"]["state"], "active", "scenario 2");
    let one = scene
        .run("syncRules.status", json!({ "id": id }))
        .await
        .expect("status");
    assert_eq!(one["rule"]["placeA"]["path"], "/home/a");
    let missing = scene
        .run("syncRules.status", json!({ "id": "nope" }))
        .await
        .expect_err("unknown");
    assert_eq!(missing.code, "sync_rules_not_found");
    assert!(scene.asked().is_empty(), "no question for reading");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rule_comes_into_being_only_after_the_user_confirms_it() {
    let scene = Scene::new();
    scene.permit(AgentFileStatus::Granted);
    let mut wanted = draft("/home/a", "/home/b");
    wanted.direction = SyncRuleDirection::Both;
    wanted.delete_mode = SyncRuleDeleteMode::Mirror;
    wanted.interval_secs = 3600;

    scene.answer(false);
    let refused = scene
        .run("syncRules.create", json_of(&wanted))
        .await
        .expect_err("declined");
    assert_eq!(refused.code, "denied_by_user");
    assert!(scene.rules().await.is_empty());
    assert!(scene.session().is_empty(), "the session heard nothing");

    scene.answer(true);
    let created = scene
        .run("syncRules.create", json_of(&wanted))
        .await
        .expect("confirmed (scenario 3)");
    assert_eq!(created["rule"]["direction"], "both");
    assert_eq!(scene.rules().await, ["Fotos"]);
    assert_eq!(scene.session(), ["reload:-"], "the new rule starts");

    let asked = scene.asked();
    assert_eq!(asked.len(), 2, "asked each time");
    let request = &asked[1];
    assert_eq!(request.action, SyncRuleConfirmAction::Create);
    assert_eq!(request.agent_id, permissions::BUILTIN_AGENT);
    assert_eq!(request.rule.place_a.path, "/home/a");
    assert_eq!(request.rule.place_b.path, "/home/b");
    assert_eq!(request.rule.direction, SyncRuleDirection::Both);
    assert_eq!(request.rule.delete_mode, SyncRuleDeleteMode::Mirror);
    assert_eq!(request.rule.interval_secs, 3600);
    assert!(request.before.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_shows_the_old_values_and_waits_for_the_user() {
    let scene = Scene::new();
    let id = scene.stored("/home/a", "/home/b").await;
    scene.permit(AgentFileStatus::Granted);
    let mut changed = draft("/home/a", "/home/b");
    changed.delete_mode = SyncRuleDeleteMode::Mirror;
    let input = json!({ "id": id, "draft": json_of(&changed) });

    scene.answer(false);
    scene
        .run("syncRules.update", input.clone())
        .await
        .expect_err("declined");
    let kept = commands::list(&scene.db, DEVICE, None).await.expect("list");
    assert_eq!(kept[0].rule.delete_mode, SyncRuleDeleteMode::Keep);
    assert!(scene.session().is_empty(), "no run was stopped");

    scene.answer(true);
    let updated = scene
        .run("syncRules.update", input)
        .await
        .expect("confirmed");
    assert_eq!(updated["rule"]["deleteMode"], "mirror");
    assert_eq!(
        scene.session(),
        [format!("stop:{id}"), format!("reload:{id}")]
    );

    let request = &scene.asked()[1];
    assert_eq!(request.action, SyncRuleConfirmAction::Update);
    assert_eq!(request.rule.delete_mode, SyncRuleDeleteMode::Mirror);
    let before = request.before.as_ref().expect("the old values");
    assert_eq!(before.delete_mode, SyncRuleDeleteMode::Keep);
    assert_eq!(before.place_a.path, "/home/a");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rule_goes_only_after_the_user_confirms() {
    let scene = Scene::new();
    let id = scene.stored("/home/a", "/home/b").await;
    scene.permit(AgentFileStatus::Granted);

    scene.answer(false);
    scene
        .run("syncRules.delete", json!({ "id": id }))
        .await
        .expect_err("declined");
    assert_eq!(scene.rules().await, ["Fotos"]);

    scene.answer(true);
    scene
        .run("syncRules.delete", json!({ "id": id }))
        .await
        .expect("confirmed");
    assert!(scene.rules().await.is_empty());
    let request = &scene.asked()[1];
    assert_eq!(request.action, SyncRuleConfirmAction::Delete);
    assert_eq!(request.rule.name, "Fotos");
    assert_eq!(request.rule.place_b.path, "/home/b");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_change_asks_whatever_the_approval_mode() {
    let scene = Scene::new();
    scene.permit(AgentFileStatus::Granted);
    for (index, mode) in [PermissionMode::Manual, PermissionMode::Auto]
        .into_iter()
        .enumerate()
    {
        let mut wanted = draft(&format!("/home/a{index}"), &format!("/home/b{index}"));
        wanted.name = format!("Regel {index}");
        scene
            .run_in(
                mode,
                "syncRules.create",
                json_of(&wanted),
                &CancellationToken::new(),
            )
            .await
            .expect("confirmed");
        assert_eq!(scene.asked().len(), index + 1, "{mode:?}");
    }
    // The chat does not ask a second time for what asks itself; reading asks neither.
    let confirming = scene.agent.confirming_action_ids();
    for action in ["syncRules.create", "syncRules.update", "syncRules.delete"] {
        assert!(confirming.contains(&action), "{action}");
    }
    for action in ["syncRules.list", "syncRules.status"] {
        assert!(!confirming.contains(&action), "{action}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn no_answer_or_a_cancelled_turn_changes_nothing() {
    let scene = Scene::with_confirm(
        SyncRuleConfirm::with_timeout(Duration::from_millis(50)),
        false,
    );
    scene.permit(AgentFileStatus::Granted);
    let timed_out = scene
        .run("syncRules.create", json_of(&draft("/home/a", "/home/b")))
        .await
        .expect_err("no answer");
    assert_eq!(timed_out.code, "denied_by_user");

    let scene = Scene::with_confirm(SyncRuleConfirm::with_timeout(Duration::from_secs(5)), false);
    scene.permit(AgentFileStatus::Granted);
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        stop.cancel();
    });
    let cancelled = scene
        .run_in(
            PermissionMode::Manual,
            "syncRules.create",
            json_of(&draft("/home/a", "/home/b")),
            &cancel,
        )
        .await
        .expect_err("cancelled");
    assert_eq!(cancelled.code, "denied_by_user");
    assert!(scene.rules().await.is_empty());
    assert_eq!(scene.asked().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn holzis_own_places_and_contacts_shares_are_refused_before_asking() {
    let scene = Scene::new();
    scene.permit(AgentFileStatus::Granted);
    let own = draft(&format!("{OWN}/vaults"), "/home/b");
    let share = SyncRuleDraft {
        place_b: SyncRulePlace {
            source: SourceRef::Storage {
                storage_id: "share:abc".to_owned(),
            },
            path: "/".to_owned(),
        },
        ..draft("/home/a", "/home/b")
    };
    for wanted in [own, share] {
        let refused = scene
            .run("syncRules.create", json_of(&wanted))
            .await
            .expect_err("scenario 5");
        assert_eq!(refused.code, "sync_rules_invalid_place");
    }
    let id = scene.stored("/home/a", "/home/b").await;
    let refused = scene
        .run(
            "syncRules.update",
            json!({ "id": id, "draft": json_of(&draft(OWN, "/home/b")) }),
        )
        .await
        .expect_err("own place");
    assert_eq!(refused.code, "sync_rules_invalid_place");
    assert!(scene.asked().is_empty(), "refused before asking");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rule_that_cannot_be_stored_is_refused_before_asking() {
    let scene = Scene::new();
    scene.permit(AgentFileStatus::Granted);
    scene.stored("/home/a", "/home/b").await;
    let refused = scene
        .run(
            "syncRules.create",
            json_of(&draft("/home/a/sub", "/home/c")),
        )
        .await
        .expect_err("overlap");
    assert_eq!(refused.code, "sync_rules_overlap");
    let bad = scene
        .run("syncRules.create", json!({ "name": "x" }))
        .await
        .expect_err("not a rule");
    assert_eq!(bad.code, "invalid_input");
    assert!(scene.asked().is_empty());
}

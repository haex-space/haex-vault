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

    fn permit(&self, on: bool) {
        crate::agent_profiles::test_support::edit_standard(&self.db, move |rights| {
            rights.files.sync_rules = on;
        });
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
    async fn stored(&self, source: &str, target: &str) -> String {
        commands::create(
            &self.db,
            DEVICE,
            &OwnPlaces::new(vec![]),
            draft(source, target),
        )
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

fn draft(source: &str, target: &str) -> SyncRuleDraft {
    let place = |path: &str| SyncRulePlace {
        location: SourceRef::Device,
        path: path.to_owned(),
    };
    SyncRuleDraft {
        name: "Fotos".to_owned(),
        source: place(source),
        target: place(target),
        source_kind: None,
        direction: SyncRuleDirection::SourceToTarget,
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
    for on in [None, Some(false)] {
        if let Some(on) = on {
            scene.permit(on);
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
    scene.permit(true);
    let list = scene.run("syncRules.list", json!({})).await.expect("list");
    assert_eq!(list[0]["rule"]["id"], id.as_str());
    assert_eq!(list[0]["status"]["state"], "active", "scenario 2");
    let one = scene
        .run("syncRules.status", json!({ "id": id }))
        .await
        .expect("status");
    assert_eq!(one["rule"]["source"]["path"], "/home/a");
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
    scene.permit(true);
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
    assert_eq!(request.agent_id, crate::files::permissions::BUILTIN_AGENT);
    assert_eq!(request.rule.source.path, "/home/a");
    assert_eq!(request.rule.target.path, "/home/b");
    assert_eq!(request.rule.direction, SyncRuleDirection::Both);
    assert_eq!(request.rule.delete_mode, SyncRuleDeleteMode::Mirror);
    assert_eq!(request.rule.interval_secs, 3600);
    assert!(request.before.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_shows_the_old_values_and_waits_for_the_user() {
    let scene = Scene::new();
    let id = scene.stored("/home/a", "/home/b").await;
    scene.permit(true);
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
    assert_eq!(before.source.path, "/home/a");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rule_goes_only_after_the_user_confirms() {
    let scene = Scene::new();
    let id = scene.stored("/home/a", "/home/b").await;
    scene.permit(true);

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
    assert_eq!(request.rule.target.path, "/home/b");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_change_asks_whatever_the_approval_mode() {
    let scene = Scene::new();
    scene.permit(true);
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
    scene.permit(true);
    let timed_out = scene
        .run("syncRules.create", json_of(&draft("/home/a", "/home/b")))
        .await
        .expect_err("no answer");
    assert_eq!(timed_out.code, "denied_by_user");

    let scene = Scene::with_confirm(SyncRuleConfirm::with_timeout(Duration::from_secs(5)), false);
    scene.permit(true);
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
    scene.permit(true);
    let own = draft(&format!("{OWN}/vaults"), "/home/b");
    let share = SyncRuleDraft {
        target: SyncRulePlace {
            location: SourceRef::Storage {
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
    scene.permit(true);
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

#[tokio::test(flavor = "multi_thread")]
async fn a_call_with_the_old_place_names_is_refused_and_changes_nothing() {
    // Spec 075 FR-006: no alias for the names before source and target. The draft is complete, so
    // the missing `source` and `target` refuse the call before anything is asked or stored.
    let scene = Scene::new();
    scene.permit(true);
    let mut old = json_of(&draft("/home/a", "/home/b"));
    let fields = old.as_object_mut().expect("a draft is an object");
    let (first, second) = (fields.remove("source"), fields.remove("target"));
    fields.insert("placeA".to_owned(), first.expect("source"));
    fields.insert("placeB".to_owned(), second.expect("target"));
    fields.insert("direction".to_owned(), json!("aToB"));

    let refused = scene
        .run("syncRules.create", old.clone())
        .await
        .expect_err("old names");
    assert_eq!(refused.code, "invalid_input");
    let id = scene.stored("/home/a", "/home/b").await;
    let refused = scene
        .run("syncRules.update", json!({ "id": id, "draft": old }))
        .await
        .expect_err("old names");
    assert_eq!(refused.code, "invalid_input");
    assert!(scene.asked().is_empty(), "nobody was asked");
    assert_eq!(
        scene.rules().await,
        ["Fotos"],
        "only the stored rule exists"
    );
}

// ---- the kind of the source (spec 074 research R2, R9) ----

use crate::file_sync::model::SyncRuleSourceKind;

fn without_the_kind(draft: &SyncRuleDraft) -> Value {
    let mut value = json_of(draft);
    value
        .as_object_mut()
        .expect("an object")
        .remove("sourceKind");
    value
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_rule_is_created_through_the_tool_and_the_question_names_the_kind() {
    let scene = Scene::new();
    scene.permit(true);
    scene.answer(true);
    let mut wanted = draft("/home/a/Haushalt.ods", "/home/b");
    wanted.source_kind = Some(SyncRuleSourceKind::File);
    wanted.delete_mode = SyncRuleDeleteMode::Mirror;
    let created = scene
        .run("syncRules.create", json_of(&wanted))
        .await
        .expect("confirmed");
    assert_eq!(created["rule"]["sourceKind"], "file");
    assert_eq!(
        created["rule"]["deleteMode"], "keep",
        "a file rule has no delete mode"
    );
    let asked = scene.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].rule.source_kind, Some(SyncRuleSourceKind::File));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_create_without_the_kind_makes_a_folder_rule() {
    let scene = Scene::new();
    scene.permit(true);
    scene.answer(true);
    let created = scene
        .run(
            "syncRules.create",
            without_the_kind(&draft("/home/a", "/home/b")),
        )
        .await
        .expect("confirmed");
    assert_eq!(created["rule"]["sourceKind"], "folder");
    assert_eq!(
        scene.asked()[0].rule.source_kind,
        Some(SyncRuleSourceKind::Folder),
        "the question shows the kind that is stored"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_update_without_the_kind_keeps_a_file_rule_a_file_rule() {
    let scene = Scene::new();
    scene.permit(true);
    scene.answer(true);
    let mut file = draft("/home/a/Haushalt.ods", "/home/b");
    file.source_kind = Some(SyncRuleSourceKind::File);
    let id = commands::create(&scene.db, DEVICE, &OwnPlaces::new(vec![]), file.clone())
        .await
        .expect("stored")
        .id;

    // An agent changes the name and leaves the kind out: the rule stays a file rule.
    let mut renamed = file.clone();
    renamed.name = "Haushalt".to_owned();
    let updated = scene
        .run(
            "syncRules.update",
            json!({ "id": id, "draft": without_the_kind(&renamed) }),
        )
        .await
        .expect("confirmed");
    assert_eq!(updated["rule"]["sourceKind"], "file");
    assert_eq!(updated["rule"]["name"], "Haushalt");
    let asked = scene.asked();
    assert_eq!(asked[0].rule.source_kind, Some(SyncRuleSourceKind::File));
    assert_eq!(
        asked[0]
            .before
            .as_ref()
            .and_then(|before| before.source_kind),
        Some(SyncRuleSourceKind::File),
        "the question shows no change of the kind"
    );

    // Naming the kind changes it.
    let mut folder = draft("/home/a", "/home/b");
    folder.name = "Haushalt".to_owned();
    folder.source_kind = Some(SyncRuleSourceKind::Folder);
    let changed = scene
        .run(
            "syncRules.update",
            json!({ "id": id, "draft": json_of(&folder) }),
        )
        .await
        .expect("confirmed");
    assert_eq!(changed["rule"]["sourceKind"], "folder");
}

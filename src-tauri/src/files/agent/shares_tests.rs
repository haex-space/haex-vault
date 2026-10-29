//! Agents managing file shares (spec 056 US6, T074 scenarios 4–6, SC-010): nothing without
//! "Dateifreigaben verwalten"; giving away waits for the user's confirmation, and without it no
//! share comes into being; taking away runs without a question.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::{SharesAgent, SharesEnv};
use crate::chat::tools::native_action::{NativeError, NativeExecutor};
use crate::files::agent::share_confirm::{FileShareConfirmRequest, ShareConfirm};
use crate::files::local::OwnPlaces;
use crate::files::peer::test_support::{store_card, Device};
use crate::files::permissions::{self, AgentFileKind, AgentFilePermission, AgentFileStatus};
use crate::files::share::FileShareLevel;
use crate::sync::keys::hex;
use crate::sync::test_support::Member;
use crate::vault_gate::VaultDb;

struct FakeSharesEnv {
    db: VaultDb,
    device: String,
    own: OwnPlaces,
    confirm: ShareConfirm,
}

#[async_trait]
impl SharesEnv for FakeSharesEnv {
    async fn vault(&self) -> Result<(VaultDb, String), crate::files::FilesError> {
        Ok((self.db.clone(), self.device.clone()))
    }

    fn own(&self) -> OwnPlaces {
        self.own.clone()
    }

    fn confirm(&self) -> &ShareConfirm {
        &self.confirm
    }
}

/// Anna's device with Ben's card, an agent over it whose window answers every confirmation with
/// `answer`, and the confirmations it saw.
struct Scene {
    anna: Device,
    ben: Device,
    agent: SharesAgent<FakeSharesEnv>,
    asked: Arc<Mutex<Vec<FileShareConfirmRequest>>>,
}

impl Scene {
    async fn new(answer: bool) -> Self {
        let anna = Device::of(Member::genesis()).await;
        let ben = Device::of(Member::genesis()).await;
        store_card(&ben, &anna).await;
        anna.write("Fotos/a.jpg", b"a");
        let confirm = ShareConfirm::with_timeout(Duration::from_secs(5));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (seen, answering) = (Arc::clone(&asked), confirm.clone());
        confirm.set_emitter(Arc::new(move |request: &FileShareConfirmRequest| {
            seen.lock().expect("asked").push(request.clone());
            answering.answer(&request.request_id, answer);
            true
        }));
        let agent = SharesAgent::builtin(FakeSharesEnv {
            db: anna.db.clone(),
            device: anna.uuid.clone(),
            own: OwnPlaces::new(vec![anna.own_place()]),
            confirm,
        });
        Self {
            anna,
            ben,
            agent,
            asked,
        }
    }

    fn permit(&self, kind: AgentFileKind, target: &str, status: AgentFileStatus) {
        let row = AgentFilePermission {
            agent_id: permissions::BUILTIN_AGENT.to_owned(),
            kind,
            target: target.to_owned(),
            status,
            updated_at: 1,
            reach: None,
        };
        self.anna
            .db
            .write_blocking(move |tx| Ok(permissions::set(tx, &row)?))
            .expect("permission");
    }

    /// "Dateifreigaben verwalten" and reading every contact.
    fn manage(&self) {
        self.permit(AgentFileKind::FileShares, "", AgentFileStatus::Granted);
        self.permit(AgentFileKind::Contacts, "*", AgentFileStatus::Read);
    }

    fn deny_contacts(&self) {
        self.permit(AgentFileKind::Contacts, "*", AgentFileStatus::Denied);
    }

    async fn run(&self, action: &str, input: Value) -> Result<Value, NativeError> {
        self.agent
            .run(action, input, &builtin_call(), &CancellationToken::new())
            .await
            .map(|reply| reply.result)
    }

    fn save(&self, share_id: Option<&str>, level: FileShareLevel) -> Value {
        json!({
            "shareId": share_id,
            "source": { "kind": "device" },
            "path": self.anna.root().join("Fotos").to_string_lossy(),
            "name": "Fotos",
            "grants": [{ "vaultPubkey": hex(&self.ben.member.vault), "level": level }],
        })
    }

    async fn shares(&self) -> Vec<Value> {
        self.manage();
        self.run("files.shares.list", json!({}))
            .await
            .expect("list")
            .as_array()
            .expect("a list")
            .clone()
    }

    fn asked(&self) -> Vec<FileShareConfirmRequest> {
        self.asked.lock().expect("asked").clone()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn without_the_permission_every_share_action_is_refused() {
    let scene = Scene::new(true).await;
    scene.permit(AgentFileKind::Contacts, "*", AgentFileStatus::Read);
    for (action, input) in [
        ("files.shares.list", json!({})),
        ("files.shares.save", scene.save(None, FileShareLevel::Read)),
    ] {
        let refused = scene.run(action, input).await.expect_err(action);
        assert_eq!(refused.code, "files_not_granted", "{action}");
        assert!(
            refused.message.contains("Dateifreigaben verwalten"),
            "names the permission (scenario 4)"
        );
    }
    assert!(scene.asked().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_share_comes_into_being_without_the_users_confirmation() {
    let scene = Scene::new(false).await;
    scene.manage();
    let refused = scene
        .run("files.shares.save", scene.save(None, FileShareLevel::Read))
        .await
        .expect_err("not confirmed");
    assert_eq!(refused.code, "denied_by_user");
    assert!(scene.shares().await.is_empty(), "SC-010");
    let asked = scene.asked();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].new_share);
    assert_eq!(asked[0].grants[0].level, FileShareLevel::Read);
    assert!(!asked[0].grants[0].contact_name.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_confirmed_share_is_made_and_taking_away_asks_nothing() {
    let scene = Scene::new(true).await;
    scene.manage();
    let share = scene
        .run(
            "files.shares.save",
            scene.save(None, FileShareLevel::ReadWrite),
        )
        .await
        .expect("confirmed (scenario 5)");
    let id = share["id"].as_str().expect("an id").to_owned();
    assert_eq!(scene.asked().len(), 1);

    scene
        .run(
            "files.shares.save",
            scene.save(Some(&id), FileShareLevel::Read),
        )
        .await
        .expect("lower");
    scene
        .run(
            "files.shares.revoke",
            json!({ "shareId": id, "vaultPubkey": hex(&scene.ben.member.vault) }),
        )
        .await
        .expect("revoked (scenario 6)");
    assert_eq!(scene.asked().len(), 1, "no question for taking away");

    scene
        .run(
            "files.shares.save",
            scene.save(Some(&id), FileShareLevel::Read),
        )
        .await
        .expect("the contact again, confirmed");
    assert_eq!(scene.asked().len(), 2, "adding a contact asks");
    scene
        .run("files.shares.delete", json!({ "shareId": id }))
        .await
        .expect("deleted");
    assert!(scene.shares().await.is_empty());
    assert_eq!(scene.asked().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn share_results_redact_contacts_the_agent_can_no_longer_read() {
    let scene = Scene::new(true).await;
    scene.manage();
    scene
        .run("files.shares.save", scene.save(None, FileShareLevel::Read))
        .await
        .expect("confirmed");

    scene.deny_contacts();
    let shares = scene
        .run("files.shares.list", json!({}))
        .await
        .expect("list")
        .as_array()
        .expect("a list")
        .clone();
    let grant = &shares[0]["grants"][0];
    assert!(grant["contactId"].is_null());
    assert!(grant["contactName"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contact_the_agent_may_not_read_is_never_offered() {
    let scene = Scene::new(true).await;
    scene.permit(AgentFileKind::FileShares, "", AgentFileStatus::Granted);
    let refused = scene
        .run("files.shares.save", scene.save(None, FileShareLevel::Read))
        .await
        .expect_err("no contacts permission");
    assert_eq!(refused.code, "files_not_granted");
    assert!(scene.asked().is_empty(), "refused before asking");
}

fn builtin_call() -> crate::chat::tools::native_action::NativeCall {
    crate::chat::tools::native_action::NativeCall::builtin(
        crate::chat::tools::permission::PermissionMode::Manual,
    )
}

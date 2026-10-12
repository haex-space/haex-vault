//! Agents and encrypted folders (spec 048 US6, T051 and the rules of T052 to T054): without a grant
//! an agent learns only "verschlüsselter Ordner <n>" with an opaque name (SC-008), the question
//! names folder, model and locality, an answer without "Erlaubnis merken" holds for its task only,
//! a grant for local models asks again for a cloud model, and nothing leaves the encryption.

use serde_json::{json, Value};

use super::prompt::{FilesAgentAnswer, FilesAgentChoice};
use super::test_env::{names, Scene};
use crate::chat::tools::agent_task::{within, AgentTask};
use crate::chat::tools::native_action::NativeError;
use crate::files::access::{Locality, StorageGrant};
use crate::files::encrypted::test_support::{filled, tree, BUCKET};
use crate::files::permissions::{AgentFileReach, AgentFileStatus};

/// Everything inside "Unterlagen", and one ordinary file beside it.
async fn storage() -> crate::files::encrypted::test_support::Scene {
    let encrypted = filled(&tree(&[
        ("brief.txt", "Steuerbescheid 2025"),
        ("Belege/strom.pdf", "%PDF-1.4"),
    ]))
    .await;
    encrypted.fake.insert(BUCKET, "offen.txt", b"offen");
    encrypted
}

/// The words of the folder an agent without a grant may never read.
const SECRETS: [&str; 6] = [
    "Unterlagen",
    "brief",
    "Steuerbescheid",
    "Belege",
    "strom",
    ".hxef",
];

fn assert_secret_free(what: &str, text: &str) {
    for secret in SECRETS {
        assert!(!text.contains(secret), "{what} names {secret}: {text}");
    }
}

fn answer(choice: FilesAgentChoice, remember: bool, also_cloud: bool) -> FilesAgentAnswer {
    FilesAgentAnswer {
        choice,
        remember,
        also_cloud,
    }
}

fn task(id: &str, locality: Locality) -> AgentTask {
    AgentTask {
        id: id.to_owned(),
        model: "qwen3-4b".to_owned(),
        locality,
    }
}

async fn run(scene: &Scene, action: &str, input: Value) -> Result<Value, NativeError> {
    scene.run(action, input).await
}

fn list(path: &str) -> Value {
    json!({ "source": "storage:s1", "path": path })
}

/// The opaque path of the one hidden folder of the top level.
async fn hidden_folder(scene: &Scene) -> String {
    let top = run(scene, "files.list", list("/")).await.expect("list");
    let hidden: Vec<&Value> = top["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|entry| entry["encrypted"] == json!(true))
        .collect();
    assert_eq!(hidden.len(), 1, "{top}");
    assert_eq!(hidden[0]["name"], "verschlüsselter Ordner 1");
    let path = hidden[0]["path"].as_str().expect("a path").to_owned();
    assert!(path.ends_with(hidden[0]["ref"].as_str().expect("a ref")));
    path
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_grant_the_agent_learns_only_that_an_encrypted_folder_exists() {
    let encrypted = storage().await;
    let scene = Scene::over_encrypted(&encrypted, StorageGrant::Read, None);
    let raw = encrypted
        .keys()
        .into_iter()
        .find(|key| key.ends_with(".hxef/h"))
        .expect("a head")
        .trim_end_matches("/h")
        .to_owned();

    let top = run(&scene, "files.list", list("/")).await.expect("list");
    assert_eq!(
        names(&top, "entries"),
        ["offen.txt", "verschlüsselter Ordner 1"]
    );
    assert_secret_free("the list", &top.to_string());
    let hidden = hidden_folder(&scene).await;

    // By its real name or its raw prefix: not found, and nobody is asked.
    for (action, input) in [
        ("files.list", list("/Unterlagen")),
        ("files.stat", list("/Unterlagen/brief.txt")),
        ("files.read", list("/Unterlagen/brief.txt")),
        ("files.list", list(&format!("/{raw}"))),
        ("files.stat", list(&format!("/{raw}/h"))),
        ("files.list", list(&format!("/{raw}/m"))),
    ] {
        let error = run(&scene, action, input.clone())
            .await
            .expect_err("refused");
        assert_secret_free(action, &format!("{} {}", error.code, error.message));
    }
    assert_eq!(scene.asked(), 0);

    // A search over the whole storage finds nothing of the folder, not even by its name.
    for query in ["brief", "Unterlagen", "strom"] {
        let found = run(
            &scene,
            "files.search",
            json!({ "source": "storage:s1", "path": "/", "query": query }),
        )
        .await
        .expect("search");
        assert_eq!(found["hits"], json!([]), "{query}: {found}");
    }

    // By its opaque name the agent may ask; without a window the answer is no.
    let error = run(&scene, "files.list", list(&hidden))
        .await
        .expect_err("refused");
    assert_eq!(error.code, "files_not_granted");
    assert_secret_free("the refusal", &error.message);
    let questions = scene.questions();
    assert_eq!(questions.len(), 1);
    assert_eq!(
        questions[0].folder.as_ref().expect("a folder").name,
        "Unterlagen"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_without_remember_holds_for_its_task_and_reading_never_writes() {
    let encrypted = storage().await;
    let scene = Scene::over_encrypted(
        &encrypted,
        StorageGrant::ReadWrite,
        Some(answer(FilesAgentChoice::Read, false, false)),
    );
    within(task("turn-1", Locality::Local), async {
        let hidden = hidden_folder(&scene).await;
        let inside = run(&scene, "files.list", list(&hidden))
            .await
            .expect("list");
        assert_eq!(names(&inside, "entries"), ["Belege", "brief.txt"]);
        let read = run(&scene, "files.read", list("/Unterlagen/brief.txt"))
            .await
            .expect("read");
        assert!(read.to_string().contains("Steuerbescheid 2025"), "{read}");
        // Granted now: the top level and a search show it by its name, without a new question.
        let top = run(&scene, "files.list", list("/")).await.expect("list");
        assert_eq!(names(&top, "entries"), ["Unterlagen", "offen.txt"]);
        let found = run(
            &scene,
            "files.search",
            json!({ "source": "storage:s1", "path": "/", "query": "strom" }),
        )
        .await
        .expect("search");
        assert_eq!(names(&found, "hits"), ["strom.pdf"]);
        // Reading never writes, and asks no more.
        let error = run(
            &scene,
            "files.folder.create",
            json!({ "source": "storage:s1", "path": "/Unterlagen", "name": "Neu" }),
        )
        .await
        .expect_err("read only");
        assert_eq!(error.code, "files_read_only");
    })
    .await;
    let questions = scene.questions();
    assert_eq!(questions.len(), 1);
    let folder = questions[0].folder.as_ref().expect("a folder");
    assert_eq!(
        (folder.name.as_str(), folder.model.as_str(), folder.cloud),
        ("Unterlagen", "qwen3-4b", false)
    );
    assert!(scene
        .agent
        .env
        .stored_folders
        .lock()
        .expect("stored")
        .is_empty());

    // The next turn knows neither the answer nor the opaque name.
    within(task("turn-2", Locality::Local), async {
        let top = run(&scene, "files.list", list("/")).await.expect("list");
        assert_secret_free("the next turn", &top.to_string());
        let hidden = hidden_folder(&scene).await;
        run(&scene, "files.list", list(&hidden))
            .await
            .expect("asked again");
    })
    .await;
    assert_eq!(scene.questions().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_remembered_grant_for_local_models_asks_again_for_a_cloud_model() {
    let encrypted = storage().await;
    let scene = Scene::over_encrypted(
        &encrypted,
        StorageGrant::ReadWrite,
        Some(answer(FilesAgentChoice::ReadWrite, true, false)),
    );
    within(task("turn-1", Locality::Local), async {
        let hidden = hidden_folder(&scene).await;
        run(&scene, "files.list", list(&hidden))
            .await
            .expect("list");
    })
    .await;
    let stored = scene
        .agent
        .env
        .stored_folders
        .lock()
        .expect("stored")
        .clone();
    assert_eq!(stored.len(), 1);
    assert_eq!(
        (stored[0].1, stored[0].2),
        (AgentFileStatus::ReadWrite, AgentFileReach::Local)
    );

    // Another local turn: granted, no question; it may write.
    within(task("turn-2", Locality::Local), async {
        let top = run(&scene, "files.list", list("/")).await.expect("list");
        assert_eq!(names(&top, "entries"), ["Unterlagen", "offen.txt"]);
        run(
            &scene,
            "files.folder.create",
            json!({ "source": "storage:s1", "path": "/Unterlagen", "name": "Neu" }),
        )
        .await
        .expect("write");
    })
    .await;
    assert_eq!(scene.questions().len(), 1);

    // A cloud model: hidden again, and its access asks anew, naming the cloud.
    within(task("turn-3", Locality::Cloud), async {
        let hidden = hidden_folder(&scene).await;
        run(&scene, "files.list", list(&hidden))
            .await
            .expect("list");
    })
    .await;
    let questions = scene.questions();
    assert_eq!(questions.len(), 2);
    assert!(questions[1].folder.as_ref().expect("a folder").cloud);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_grant_stays_within_the_storage_grant_and_nothing_leaves_the_encryption() {
    let encrypted = storage().await;
    let reading = Scene::over_encrypted(
        &encrypted,
        StorageGrant::Read,
        Some(answer(FilesAgentChoice::ReadWrite, false, false)),
    );
    within(task("turn-1", Locality::Local), async {
        let hidden = hidden_folder(&reading).await;
        run(&reading, "files.list", list(&hidden))
            .await
            .expect("list");
        let error = run(
            &reading,
            "files.folder.create",
            json!({ "source": "storage:s1", "path": "/Unterlagen", "name": "Neu" }),
        )
        .await
        .expect_err("the storage is read only");
        assert_eq!(error.code, "files_not_granted");
    })
    .await;

    let writing = Scene::over_encrypted(
        &encrypted,
        StorageGrant::ReadWrite,
        Some(answer(FilesAgentChoice::ReadWrite, false, false)),
    );
    within(task("turn-1", Locality::Local), async {
        let hidden = hidden_folder(&writing).await;
        run(&writing, "files.list", list(&hidden))
            .await
            .expect("list");
        let error = run(
            &writing,
            "files.copy",
            json!({ "source": "storage:s1", "paths": ["/Unterlagen/brief.txt"], "to": "/" }),
        )
        .await
        .expect_err("leaves the encryption");
        assert_eq!(error.code, "files_not_granted");
    })
    .await;
    assert!(!encrypted.keys().contains(&"brief.txt".to_owned()));
}

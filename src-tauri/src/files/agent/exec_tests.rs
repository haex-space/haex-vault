//! The reading file actions of the built-in agent (contracts/agent-actions.md) and the catalogue of
//! SC-005: holzi's own places are never reached, by direct path, `..`, a link or a search, and a
//! storage without a grant stays unknown.

use serde_json::json;

use super::prompt::{FilesAgentChoice, FilesAgentWant};
use super::test_env::{names, Scene};
use crate::files::access::{DeviceGrant, StorageGrant};
use crate::files::permissions::AgentFileStatus;

fn code(
    result: Result<serde_json::Value, crate::chat::tools::native_action::NativeError>,
) -> String {
    result.expect_err("refused").code
}

#[tokio::test]
async fn sources_name_the_device_and_only_granted_storages() {
    let scene = Scene::new(None);
    let sources = scene
        .run("files.sources", json!({}))
        .await
        .expect("sources");
    assert_eq!(sources["device"]["source"], "device");
    assert_eq!(sources["device"]["places"][0]["name"], "Steuer");
    assert_eq!(sources["storages"], json!([]));

    scene
        .agent
        .env
        .grants
        .lock()
        .unwrap()
        .storages
        .insert("s1".into(), StorageGrant::Read);
    let sources = scene
        .run("files.sources", json!({}))
        .await
        .expect("sources");
    assert_eq!(
        sources["storages"],
        json!([{ "source": "storage:s1", "name": "Fotos", "access": "read" }])
    );
}

#[tokio::test]
async fn a_list_leaves_holzis_own_places_out() {
    let scene = Scene::new(None);
    let list = scene
        .run(
            "files.list",
            json!({ "source": "device", "path": scene.at("") }),
        )
        .await
        .expect("list");
    assert_eq!(
        names(&list, "entries"),
        ["b", "bin.dat", "foto.jpg", "notiz.txt", "steuer"]
    );
    assert_eq!(list["truncated"], false);
    let list = scene
        .run(
            "files.list",
            json!({ "source": "device", "path": scene.at(""), "limit": 2 }),
        )
        .await
        .expect("list");
    assert_eq!(list["entries"].as_array().unwrap().len(), 2);
    assert_eq!(list["truncated"], true);
}

#[cfg(unix)]
#[tokio::test]
async fn links_to_holzis_own_data_are_hidden_from_list_and_search() {
    let scene = Scene::new(None);
    std::os::unix::fs::symlink(scene.path("holzi/vault.db"), scene.path("link-to-vault.db"))
        .unwrap();
    std::os::unix::fs::symlink(scene.path("holzi"), scene.path("link-to-holzi")).unwrap();

    let list = scene
        .run(
            "files.list",
            json!({ "source": "device", "path": scene.at("") }),
        )
        .await
        .expect("list");
    assert!(!names(&list, "entries")
        .iter()
        .any(|name| { name == "link-to-vault.db" || name == "link-to-holzi" }));

    let found = scene
        .run(
            "files.search",
            json!({ "source": "device", "path": scene.at(""), "query": "link-to" }),
        )
        .await
        .expect("search");
    assert!(names(&found, "hits").is_empty());
}

#[tokio::test]
async fn text_is_read_up_to_its_limit_and_other_files_get_a_note() {
    let scene = Scene::new(None);
    let read = |name: &str| {
        scene.run(
            "files.read",
            json!({ "source": "device", "path": scene.at(name) }),
        )
    };
    let text = read("notiz.txt").await.expect("text");
    assert_eq!(text["text"], "Hallo Welt");
    assert_eq!(text["truncated"], false);
    assert_eq!(text["entry"]["size"], 10);
    for name in ["foto.jpg", "bin.dat"] {
        let note = read(name).await.expect("note");
        assert!(note["note"].is_string(), "{name}");
        assert!(note.get("text").is_none(), "{name}");
    }
    assert_eq!(code(read("steuer").await), "invalid_input");

    let long = "ä".repeat(crate::files::extract::TEXT_CHARS + 5);
    std::fs::write(scene.path("lang.txt"), &long).expect("write");
    let text = read("lang.txt").await.expect("text");
    assert_eq!(
        text["text"].as_str().unwrap().chars().count(),
        crate::files::extract::TEXT_CHARS
    );
    assert_eq!(text["truncated"], true);
}

#[tokio::test]
async fn a_search_finds_names_and_never_holzis_own_files() {
    let scene = Scene::new(None);
    let found = scene
        .run(
            "files.search",
            json!({ "source": "device", "path": scene.at(""), "query": "brief" }),
        )
        .await
        .expect("search");
    assert_eq!(names(&found, "hits"), ["brief-2025.txt"]);
    assert_eq!(found["truncated"], false);
    let filtered = scene
        .run(
            "files.search",
            json!({ "source": "device", "path": scene.at(""), "query": "brief", "types": ["image"] }),
        )
        .await
        .expect("search");
    assert_eq!(names(&filtered, "hits"), Vec::<String>::new());
    assert_eq!(
        scene
            .run(
                "files.search",
                json!({ "source": "device", "path": scene.at(""), "query": "x", "modifiedAfter": "gestern" }),
            )
            .await
            .expect_err("bad date")
            .field
            .as_deref(),
        Some("modifiedAfter")
    );
}

/// SC-005: every way to holzi's own files is refused without content.
#[tokio::test]
async fn holzis_own_files_are_blocked_however_they_are_named() {
    let scene = Scene::new(None);
    // Only Unix adds the links below.
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut ways = vec![
        scene.at("holzi/vault.db"),
        scene.at("holzi"),
        scene.at("steuer/../holzi/vault.db"),
    ];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(scene.path("holzi/vault.db"), scene.path("link.db")).unwrap();
        std::os::unix::fs::symlink(scene.path("holzi"), scene.path("link-dir")).unwrap();
        ways.push(scene.at("link.db"));
        ways.push(scene.at("link-dir/vault.db"));
    }
    for path in &ways {
        for action in ["files.read", "files.stat", "files.list"] {
            let refused = scene
                .run(action, json!({ "source": "device", "path": path }))
                .await
                .expect_err("blocked");
            assert_eq!(refused.code, "files_blocked", "{action} {path}");
            assert!(!refused.message.contains("secret"), "{action} {path}");
        }
    }
    // A case variant is another name on a case-sensitive system and the same folder elsewhere:
    // either way no content.
    let variant = scene
        .run(
            "files.read",
            json!({ "source": "device", "path": scene.at("HOLZI/vault.db") }),
        )
        .await;
    assert!(variant.is_err());
}

#[tokio::test]
async fn without_the_device_grant_the_device_is_closed() {
    let scene = Scene::new(None);
    scene.agent.env.grants.lock().unwrap().device = Some(DeviceGrant::Denied);
    let refused = scene
        .run(
            "files.list",
            json!({ "source": "device", "path": scene.at("") }),
        )
        .await;
    assert_eq!(code(refused), "files_not_granted");
    let sources = scene
        .run("files.sources", json!({}))
        .await
        .expect("sources");
    assert!(sources.get("device").is_none());
}

#[tokio::test]
async fn a_storage_is_asked_for_once_and_the_answer_kept() {
    let scene = Scene::new(Some(FilesAgentChoice::Read));
    let list = scene
        .run(
            "files.list",
            json!({ "source": "storage:Fotos", "path": "/" }),
        )
        .await
        .expect("list");
    assert_eq!(names(&list, "entries"), ["a.txt"]);
    assert_eq!(scene.asked(), 1);
    assert_eq!(
        scene.agent.env.asked.lock().unwrap()[0].wants,
        FilesAgentWant::Read
    );
    assert_eq!(
        *scene.agent.env.stored.lock().unwrap(),
        [("s1".to_owned(), AgentFileStatus::Read)]
    );
    let text = scene
        .run(
            "files.read",
            json!({ "source": "storage:s1", "path": "/a.txt" }),
        )
        .await
        .expect("read");
    assert_eq!(text["text"], "im Speicher");
    assert_eq!(scene.asked(), 1, "no second question");

    // Read only: writing is refused without asking, whatever the autonomy mode.
    let refused = scene
        .run(
            "files.folder.create",
            json!({ "source": "storage:s1", "path": "/", "name": "neu" }),
        )
        .await;
    assert_eq!(code(refused), "files_not_granted");
    assert_eq!(scene.asked(), 1);
}

/// Spec 050 US2 (044 FR-019, FR-031a): a WebDAV storage is reached through the same paths as an S3
/// one: asked for once, then listed, searched and read; a read grant writes nothing.
#[tokio::test]
async fn a_webdav_storage_is_reached_like_an_s3_storage() {
    let mut scene = Scene::new(Some(FilesAgentChoice::Read));
    scene.agent.env.protocol = crate::remote_storage::Protocol::WebDav;
    scene.agent.env.store.insert("s1", "Briefe/", b"");
    scene
        .agent
        .env
        .store
        .insert("s1", "Briefe/strom.txt", b"Abschlag");
    let list = scene
        .run("files.list", json!({ "source": "storage:s1", "path": "/" }))
        .await
        .expect("list");
    assert_eq!(names(&list, "entries"), ["Briefe", "a.txt"]);
    assert_eq!(scene.asked(), 1);
    let found = scene
        .run(
            "files.search",
            json!({ "source": "storage:s1", "path": "/", "query": "strom" }),
        )
        .await
        .expect("search");
    assert_eq!(found["hits"][0]["path"], "/Briefe/strom.txt", "{found}");
    let text = scene
        .run(
            "files.read",
            json!({ "source": "storage:s1", "path": "/Briefe/strom.txt" }),
        )
        .await
        .expect("read");
    assert_eq!(text["text"], "Abschlag");
    let refused = scene
        .run(
            "files.rename",
            json!({ "source": "storage:s1", "path": "/Briefe", "newName": "Post" }),
        )
        .await;
    assert_eq!(code(refused), "files_not_granted");
    assert_eq!(scene.asked(), 1, "no second question");
}

/// SC-005: without a grant, a storage of that name and none at all look the same.
#[tokio::test]
async fn an_unknown_storage_and_a_refused_one_answer_alike() {
    for answer in [None, Some(FilesAgentChoice::Deny)] {
        let scene = Scene::new(answer);
        let refused = scene
            .run(
                "files.list",
                json!({ "source": "storage:Fotos", "path": "/" }),
            )
            .await
            .expect_err("refused");
        let unknown = scene
            .run(
                "files.list",
                json!({ "source": "storage:Urlaub", "path": "/" }),
            )
            .await
            .expect_err("unknown");
        assert_eq!(refused, unknown, "{answer:?}");
        let stored = scene.agent.env.stored.lock().unwrap().clone();
        match answer {
            None => assert!(stored.is_empty(), "no window: nothing stored"),
            Some(_) => assert_eq!(stored, [("s1".to_owned(), AgentFileStatus::Denied)]),
        }
    }
}

#[tokio::test]
async fn a_storage_error_reaches_the_agent_without_credentials() {
    let scene = Scene::new(Some(FilesAgentChoice::Read));
    scene.agent.env.store.fail(
        crate::remote_storage::test_support::Op::ListDir,
        crate::remote_storage::StorageError::AccessDenied,
    );
    let refused = scene
        .run("files.list", json!({ "source": "storage:s1", "path": "/" }))
        .await
        .expect_err("refused");
    assert_eq!(refused.code, "files_credentials");
    for secret in ["AKIDEXAMPLE", "placeholder-secret"] {
        assert!(!refused.message.contains(secret));
    }
}

#[tokio::test]
async fn files_show_names_the_folder_and_the_file() {
    let scene = Scene::new(None);
    let cancel = tokio_util::sync::CancellationToken::new();
    let (source, folder, open) = scene
        .agent
        .show_target("device", &scene.at("steuer/brief-2025.txt"), &cancel)
        .await
        .expect("allowed");
    assert_eq!(source, crate::files::SourceRef::Device);
    assert_eq!(folder, scene.at("steuer"));
    assert_eq!(open.as_deref(), Some("brief-2025.txt"));
    let (_, folder, open) = scene
        .agent
        .show_target("device", &scene.at("steuer"), &cancel)
        .await
        .expect("allowed");
    assert_eq!((folder, open), (scene.at("steuer"), None));
    let refused = scene
        .agent
        .show_target("device", &scene.at("holzi/vault.db"), &cancel)
        .await
        .expect_err("blocked");
    assert_eq!(refused.code, "files_blocked");
}

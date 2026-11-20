//! The writing file actions of the built-in agent (contracts/agent-actions.md): names taken at the
//! target decided by `onConflict`, a copy that never replaces, haex-vault's own places refused also
//! inside a folder, and storages written only with `readWrite`.

use std::fs;

use serde_json::json;

use super::prompt::{FilesAgentChoice, FilesAgentWant};
use super::test_env::Scene;
use crate::files::access::StorageGrant;

#[tokio::test]
async fn a_folder_is_created_and_an_entry_renamed() {
    let scene = Scene::new(None);
    let made = scene
        .run(
            "files.folder.create",
            json!({ "source": "device", "path": scene.at("b"), "name": "neu" }),
        )
        .await
        .expect("created");
    assert_eq!(made["kind"], "dir");
    assert!(scene.path("b/neu").is_dir());
    let renamed = scene
        .run(
            "files.rename",
            json!({ "source": "device", "path": scene.at("notiz.txt"), "newName": "memo.txt" }),
        )
        .await
        .expect("renamed");
    assert_eq!(renamed["name"], "memo.txt");
    assert!(scene.path("memo.txt").is_file());
}

#[tokio::test]
async fn nothing_in_or_around_haexs_own_places_is_changed() {
    let scene = Scene::new(None);
    let attempts = [
        (
            "files.folder.create",
            json!({ "source": "device", "path": scene.at("haex-vault"), "name": "x" }),
        ),
        (
            "files.rename",
            json!({ "source": "device", "path": scene.at("haex-vault"), "newName": "x" }),
        ),
        (
            "files.rename",
            json!({ "source": "device", "path": scene.at(""), "newName": "x" }),
        ),
        (
            "files.copy",
            json!({ "source": "device", "paths": [scene.at("haex-vault/vault.db")], "to": scene.at("b") }),
        ),
        (
            "files.copy",
            json!({ "source": "device", "paths": [scene.at("")], "to": scene.at("b") }),
        ),
        (
            "files.move",
            json!({ "source": "device", "paths": [scene.at("notiz.txt")], "to": scene.at("haex-vault") }),
        ),
        (
            "files.delete",
            json!({ "source": "device", "paths": [scene.at("haex-vault")] }),
        ),
        (
            "files.delete",
            json!({ "source": "device", "paths": [scene.at("")] }),
        ),
    ];
    for (action, input) in attempts {
        let refused = scene.run(action, input.clone()).await.expect_err("blocked");
        assert_eq!(refused.code, "files_blocked", "{action} {input}");
    }
    assert!(scene.path("haex-vault/vault.db").is_file());
    assert!(scene.path("notiz.txt").is_file());
}

#[tokio::test]
async fn a_taken_name_is_skipped_or_kept_both_and_never_replaced_by_a_copy() {
    let scene = Scene::new(None);
    fs::write(scene.path("b/notiz.txt"), "alt").unwrap();
    let copy = |on_conflict: &str| {
        scene.run(
            "files.copy",
            json!({ "source": "device", "paths": [scene.at("notiz.txt")], "to": scene.at("b"), "onConflict": on_conflict }),
        )
    };
    let skipped = copy("skip").await.expect("copied");
    assert_eq!(skipped["skipped"], 1);
    assert_eq!(
        fs::read_to_string(scene.path("b/notiz.txt")).unwrap(),
        "alt"
    );
    let kept = copy("keepBoth").await.expect("copied");
    assert_eq!(kept["keptBoth"], 1);
    assert_eq!(
        fs::read_to_string(scene.path("b/notiz (2).txt")).unwrap(),
        "Hallo Welt"
    );
    let refused = copy("replace").await.expect_err("no replace");
    assert_eq!(
        (refused.code.as_str(), refused.field.as_deref()),
        ("invalid_input", Some("onConflict"))
    );
    assert_eq!(
        fs::read_to_string(scene.path("b/notiz.txt")).unwrap(),
        "alt"
    );
}

#[tokio::test]
async fn a_move_may_replace_and_a_delete_removes() {
    let scene = Scene::new(None);
    fs::write(scene.path("b/notiz.txt"), "alt").unwrap();
    let moved = scene
        .run(
            "files.move",
            json!({ "source": "device", "paths": [scene.at("notiz.txt")], "to": scene.at("b"), "onConflict": "replace" }),
        )
        .await
        .expect("moved");
    assert_eq!(moved["replaced"], 1);
    assert_eq!(
        fs::read_to_string(scene.path("b/notiz.txt")).unwrap(),
        "Hallo Welt"
    );
    assert!(!scene.path("notiz.txt").exists());
    let deleted = scene
        .run(
            "files.delete",
            json!({ "source": "device", "paths": [scene.at("b")] }),
        )
        .await
        .expect("deleted");
    assert_eq!(deleted, json!({ "deleted": 1, "trash": false }));
    assert!(!scene.path("b").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn writing_into_a_storage_asks_for_read_and_write() {
    let scene = Scene::new(Some(FilesAgentChoice::ReadWrite));
    scene
        .run(
            "files.copy",
            json!({ "source": "device", "paths": [scene.at("notiz.txt")], "to": "/", "toSource": "storage:Fotos" }),
        )
        .await
        .expect("uploaded");
    assert_eq!(
        scene.agent.env.store.object("s1", "notiz.txt").as_deref(),
        Some(&b"Hallo Welt"[..])
    );
    assert_eq!(
        scene.agent.env.asked.lock().unwrap()[0].wants,
        FilesAgentWant::ReadWrite
    );
    let deleted = scene
        .run(
            "files.delete",
            json!({ "source": "storage:s1", "paths": ["/a.txt"] }),
        )
        .await
        .expect("deleted");
    assert_eq!(deleted, json!({ "deleted": 1, "trash": false }));
    assert!(scene.agent.env.store.object("s1", "a.txt").is_none());
    assert_eq!(scene.asked(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_storage_granted_for_reading_gives_files_but_takes_none() {
    let scene = Scene::new(None);
    scene
        .agent
        .env
        .grants
        .lock()
        .unwrap()
        .storages
        .insert("s1".into(), StorageGrant::Read);
    scene
        .run(
            "files.copy",
            json!({ "source": "storage:s1", "paths": ["/a.txt"], "to": scene.at("b"), "toSource": "device" }),
        )
        .await
        .expect("downloaded");
    assert_eq!(
        fs::read_to_string(scene.path("b/a.txt")).unwrap(),
        "im Speicher"
    );
    let refused = scene
        .run(
            "files.copy",
            json!({ "source": "device", "paths": [scene.at("notiz.txt")], "to": "/", "toSource": "storage:s1" }),
        )
        .await
        .expect_err("read only");
    assert_eq!(refused.code, "files_not_granted");
    assert_eq!(scene.asked(), 0);
}

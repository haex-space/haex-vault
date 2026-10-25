//! Agents on other devices and received shares (spec 056 US6, T074 scenarios 1–3): they are
//! storages with their own grant; on a share given for reading only the narrower level wins.

use serde_json::json;

use super::test_env::Scene;
use crate::files::access::StorageGrant;
use crate::files::browser_commands::StorageSource;

/// A scene with the other device "Laptop" and Ben's share "Rezepte" as storages.
fn scene() -> Scene {
    let mut scene = Scene::new(None);
    for (id, name) in [("peer:laptop", "Laptop"), ("share:rezepte", "Rezepte")] {
        scene.agent.env.storages.push(StorageSource {
            id: id.to_owned(),
            name: name.to_owned(),
        });
        scene.agent.env.store.insert(id, "a.txt", b"da");
    }
    scene
}

fn grant(scene: &Scene, id: &str, grant: StorageGrant) {
    scene
        .agent
        .env
        .grants
        .lock()
        .unwrap()
        .storages
        .insert(id.to_owned(), grant);
}

fn write_to(scene: &Scene, source: &str) -> serde_json::Value {
    json!({
        "source": "device",
        "paths": [scene.at("notiz.txt")],
        "to": "/",
        "toSource": source,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn another_device_without_a_grant_is_not_reached() {
    let scene = scene();
    let refused = scene
        .run(
            "files.list",
            json!({ "source": "storage:Laptop", "path": "/" }),
        )
        .await
        .expect_err("no grant, nobody to ask");
    assert_eq!(refused.code, "files_not_granted");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_grant_on_a_share_takes_nothing_even_where_the_contact_could() {
    let scene = scene();
    grant(&scene, "share:rezepte", StorageGrant::Read);
    let listed = scene
        .run(
            "files.list",
            json!({ "source": "storage:Rezepte", "path": "/" }),
        )
        .await
        .expect("read by its name");
    assert_eq!(listed["entries"][0]["name"], "a.txt");
    let refused = scene
        .run("files.copy", write_to(&scene, "storage:share:rezepte"))
        .await
        .expect_err("scenario 2");
    assert_eq!(refused.code, "files_not_granted");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_narrower_of_agent_grant_and_share_level_wins() {
    let mut scene = scene();
    scene.agent.env.read_only.push("share:rezepte".to_owned());
    grant(&scene, "share:rezepte", StorageGrant::ReadWrite);
    grant(&scene, "peer:laptop", StorageGrant::ReadWrite);
    let refused = scene
        .run("files.copy", write_to(&scene, "storage:share:rezepte"))
        .await
        .expect_err("the share is read only (FR-041)");
    assert_eq!(refused.code, "files_read_only");
    assert!(scene
        .agent
        .env
        .store
        .object("share:rezepte", "notiz.txt")
        .is_none());

    scene
        .run("files.copy", write_to(&scene, "storage:peer:laptop"))
        .await
        .expect("scenario 3");
    assert!(scene
        .agent
        .env
        .store
        .object("peer:laptop", "notiz.txt")
        .is_some());
}

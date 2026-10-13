//! Agents and encrypted folders on the device (spec 048 US7, FR-050, T079): the raw directory is
//! blocked and never listed, a folder shows opaque until granted, by its real name it is not
//! found, its grant is asked for by its opaque name, and paths into it never mix with ordinary
//! device paths in one call.

use serde_json::{json, Value};

use super::prompt::{FilesAgentAnswer, FilesAgentChoice};
use super::test_env::{names, Scene};
use crate::files::encrypted::device_scene::DeviceScene;

const SECRETS: [&str; 4] = ["Unterlagen", "notiz", "Strom", ".hxef"];

fn assert_secret_free(what: &str, text: &str) {
    for secret in SECRETS {
        assert!(!text.contains(secret), "{what} names {secret}: {text}");
    }
}

fn at(scene: &Scene, rel: &str) -> String {
    scene.root.join(rel).to_string_lossy().into_owned()
}

fn on_device(path: &str) -> Value {
    json!({ "source": "device", "path": path })
}

/// The device with "Unterlagen" (holding `notiz.txt`) and an ordinary `offen.txt`.
async fn device() -> (DeviceScene, String) {
    let device = DeviceScene::new();
    let raw = device.with_folder().await;
    std::fs::write(device.home.path().join("offen.txt"), "offen").unwrap();
    (device, raw)
}

#[tokio::test(flavor = "multi_thread")]
async fn on_the_device_an_agent_reaches_an_encrypted_folder_only_with_its_grant() {
    let (device, raw) = device().await;
    let read = FilesAgentAnswer {
        choice: FilesAgentChoice::Read,
        remember: false,
        also_cloud: true,
    };
    let scene = Scene::over_device(&device, Some(read));
    let home = scene.root.to_string_lossy().into_owned();

    // The listing: the ordinary file, and the folder opaque, nothing of its name or raw directory.
    let top = scene.run("files.list", on_device(&home)).await.unwrap();
    assert_eq!(
        names(&top, "entries"),
        ["offen.txt", "verschlüsselter Ordner 1"]
    );
    assert_secret_free("the list", &top.to_string());
    let hidden = top["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["encrypted"] == json!(true))
        .and_then(|entry| entry["path"].as_str())
        .unwrap()
        .to_owned();

    // By its real name or its raw directory: refused, and nobody is asked.
    for (action, path) in [
        ("files.list", at(&scene, "Unterlagen")),
        ("files.read", at(&scene, "Unterlagen/notiz.txt")),
        ("files.list", at(&scene, &raw)),
        ("files.read", at(&scene, &format!("{raw}/h"))),
    ] {
        let error = scene
            .run(action, on_device(&path))
            .await
            .expect_err("refused");
        assert_secret_free(action, &format!("{} {}", error.code, error.message));
    }
    assert_eq!(scene.asked(), 0);

    // By its opaque name the agent asks; with "read" it lists and reads the folder.
    let inside = scene.run("files.list", on_device(&hidden)).await.unwrap();
    assert_eq!(names(&inside, "entries"), ["notiz.txt"]);
    assert_eq!(scene.asked(), 1);
    let note = scene
        .run("files.read", on_device(&format!("{hidden}/notiz.txt")))
        .await
        .unwrap();
    assert!(note.to_string().contains("Strom"), "{note}");

    // Reading was granted, writing not; and the paths of one call never mix.
    let error = scene
        .run(
            "files.delete",
            json!({ "source": "device", "paths": [format!("{hidden}/notiz.txt")] }),
        )
        .await
        .expect_err("read only");
    assert!(error.code.starts_with("files_"), "{error:?}");
    let error = scene
        .run(
            "files.delete",
            json!({
                "source": "device",
                "paths": [at(&scene, "offen.txt"), format!("{hidden}/notiz.txt")],
            }),
        )
        .await
        .expect_err("mixed");
    assert_eq!(error.code, "invalid_input");
    assert!(scene.root.join("offen.txt").exists());
}

//! FR-027 (T067): no log line names anything of an encrypted folder. Folders on the device and in a
//! storage are created, filled, listed, read (also a damaged file), renamed, searched and deleted
//! while every log record is kept; none holds a folder or entry name, a path through the folder, or
//! the raw directory and object keys.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::super::device_scene::{walkdir, DeviceScene};
use super::super::ops::create_encrypted_folder;
use super::super::test_support::{transfer, Scene, BUCKET};
use crate::files::search::{SearchFilters, SearchLimits, SearchOptions};
use crate::files::search_device::search_passed;
use crate::files::transfer::remote::Side;
use crate::files::transfer::TransferOp;

const FOLDER: &str = "Lohnzettel Qx7";
const FILE: &str = "Vertrag Zk9.txt";
const RENAMED: &str = "Kuendigung Wy4.txt";
const CONTENT: &str = "Inhalt Vb3";

/// `FILE` with `CONTENT` in a fresh device folder, as a transfer source.
fn source() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    std::fs::write(&file, CONTENT).unwrap();
    let path = file.to_string_lossy().into_owned();
    (dir, path)
}

fn options() -> SearchOptions {
    SearchOptions {
        filters: SearchFilters::default(),
        show_hidden: false,
        own: crate::files::local::OwnPlaces::default(),
        hide_own: false,
        limits: SearchLimits::USER,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn no_log_line_names_anything_of_an_encrypted_folder() {
    crate::test_log::install();
    let cancel = CancellationToken::new();

    // On the device.
    let device = DeviceScene::new();
    let files = device.files();
    create_encrypted_folder(&files, &device.view(""), FOLDER)
        .await
        .unwrap();
    let (_dir, file) = source();
    let folder = device.view(FOLDER);
    transfer(
        TransferOp::Copy,
        Side::Device,
        std::slice::from_ref(&file),
        Side::Storage(Arc::new(device.files())),
        &folder,
        None,
        None,
    )
    .await
    .unwrap();
    files.list(&folder).await.unwrap();
    let inside = format!("{folder}/{FILE}");
    files.read_start(&inside, 64, 64).await.unwrap();
    let mut passed = Vec::new();
    crate::files::search::search_passing(
        device.home.path(),
        "Vertrag",
        &options(),
        &cancel,
        &mut |_| {},
        &mut passed,
    );
    search_passed(
        &files,
        &device.root,
        passed,
        "Vertrag",
        &options(),
        0,
        &cancel,
        &mut |_| {},
    )
    .await;
    files.rename(&inside, RENAMED).await.unwrap();
    // A damaged file: its content object cut short on the disk.
    let raw = device
        .home
        .path()
        .join(super::super::device_scene::raw_dir(device.home.path()));
    for object in walkdir(&raw.join("c")) {
        let bytes = std::fs::read(&object).unwrap();
        std::fs::write(&object, &bytes[..bytes.len() / 2]).unwrap();
    }
    let _ = device
        .files()
        .read_start(&format!("{folder}/{RENAMED}"), 64, 64)
        .await;
    files.delete(&folder, &cancel, &mut |_| {}).await.unwrap();

    // In a storage.
    let scene = Scene::new();
    let files = scene.files();
    create_encrypted_folder(&files, "/", FOLDER).await.unwrap();
    transfer(
        TransferOp::Copy,
        Side::Device,
        &[file],
        Side::Storage(Arc::new(scene.files())),
        &format!("/{FOLDER}"),
        None,
        None,
    )
    .await
    .unwrap();
    let inside = format!("/{FOLDER}/{FILE}");
    files.read_start(&inside, 64, 64).await.unwrap();
    files.rename(&inside, RENAMED).await.unwrap();
    for key in scene.keys().iter().filter(|key| key.contains("/c/")) {
        let bytes = scene.fake.object(BUCKET, key).unwrap();
        scene.fake.insert(BUCKET, key, &bytes[..bytes.len() / 2]);
    }
    let _ = scene
        .fresh_files()
        .read_start(&format!("/{FOLDER}/{RENAMED}"), 64, 64)
        .await;
    let keys = scene.keys();
    files
        .delete(&format!("/{FOLDER}"), &cancel, &mut |_| {})
        .await
        .unwrap();

    let logs = crate::test_log::all();
    let raw_name = raw.file_name().unwrap().to_string_lossy().into_owned();
    let mut secrets = vec![
        FOLDER.to_owned(),
        FILE.to_owned(),
        RENAMED.to_owned(),
        CONTENT.to_owned(),
        "Lohnzettel".to_owned(),
        "Zk9".to_owned(),
        raw_name,
    ];
    secrets.extend(keys);
    for secret in secrets {
        assert!(!logs.contains(&secret), "the log names {secret}:\n{logs}");
    }
}

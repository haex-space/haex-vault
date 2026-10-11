//! The manipulation catalogue of SC-002 (FR-022 to FR-025) on a filled folder: whatever the
//! provider changes, the affected entry is damaged, the others stay readable, and no changed
//! plaintext reaches a reader.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::format::content::HEADER_LEN;
use super::format::{decode_id, CHUNK};
use super::ops::create_encrypted_folder;
use super::test_support::{transfer, Scene, BUCKET};
use crate::files::transfer::remote::Side;
use crate::files::transfer::TransferOp;
use crate::files::{EncryptedState, FilesErrorCode};

/// What the folder "Unterlagen" holds: name → content.
fn originals() -> BTreeMap<&'static str, Vec<u8>> {
    BTreeMap::from([
        ("a.txt", b"Rechnung Strom".to_vec()),
        ("b.txt", b"Rechnung Gas!!".to_vec()),
        (
            "c.bin",
            (0..2 * CHUNK + 7).map(|i| (i % 249) as u8).collect(),
        ),
        (
            "d.bin",
            (0..2 * CHUNK + 7).map(|i| (i % 241) as u8).collect(),
        ),
    ])
}

async fn filled() -> Scene {
    let scene = Scene::new();
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    create_encrypted_folder(&files, "/", "Andere")
        .await
        .unwrap();
    files.create_folder("/Unterlagen", "Belege").await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for (name, content) in originals() {
        std::fs::write(dir.path().join(name), content).unwrap();
        paths.push(dir.path().join(name).to_string_lossy().into_owned());
    }
    paths.push({
        std::fs::write(dir.path().join("x.txt"), "anderswo").unwrap();
        dir.path().join("x.txt").to_string_lossy().into_owned()
    });
    let (into_folder, into_other) = paths.split_at(4);
    for (sources, target) in [(into_folder, "/Unterlagen"), (into_other, "/Andere")] {
        transfer(
            TransferOp::Copy,
            Side::Device,
            sources,
            Side::Storage(Arc::new(scene.files())),
            target,
            None,
            None,
        )
        .await
        .unwrap();
    }
    scene
}

/// The object key of the content of `name` in "Unterlagen" and of its sidecar.
async fn keys_of(scene: &Scene, name: &str) -> (String, String) {
    let files = scene.files();
    let super::route::Route::Encrypted(at) =
        super::route::route(&files, &format!("/Unterlagen/{name}"))
            .await
            .unwrap()
    else {
        panic!("encrypted");
    };
    let (folder, version) = super::ops::file_at(&files, &at).await.unwrap();
    let facts = super::tree::facts(&version).unwrap();
    (
        folder.content_key(&facts.content),
        folder.sidecar_key(&version.sid),
    )
}

fn edit(scene: &Scene, key: &str, change: impl FnOnce(&mut Vec<u8>)) {
    let mut body = scene.fake.object(BUCKET, key).unwrap();
    change(&mut body);
    scene.fake.insert(BUCKET, key, &body);
}

/// Opens the folder as a fresh device would: `damaged` files refuse, all others read whole and
/// unchanged, and `broken` sidecars show as damaged entries.
async fn expect(scene: &Scene, damaged: &[&str], broken: usize) {
    let files = scene.fresh_files();
    let listed = files.list("/Unterlagen").await.unwrap();
    let placeholders = listed
        .iter()
        .filter(|entry| decode_id(&entry.name).is_some())
        .inspect(|entry| {
            assert_eq!(entry.encrypted.unwrap().state, EncryptedState::Damaged);
        })
        .count();
    assert_eq!(placeholders, broken, "{listed:?}");
    for (name, content) in originals() {
        let path = format!("/Unterlagen/{name}");
        let Some(entry) = listed.iter().find(|entry| entry.name == name) else {
            assert!(damaged.contains(&name), "{name} vanished");
            continue;
        };
        let read = files.read_start(&path, 0, 10 * CHUNK as u64).await;
        if damaged.contains(&name) {
            assert_eq!(read.unwrap_err().code, FilesErrorCode::Damaged, "{name}");
            let again = files.stat(&path).await.unwrap();
            assert_eq!(again.encrypted.unwrap().state, EncryptedState::Damaged);
        } else {
            assert_eq!(entry.encrypted.unwrap().state, EncryptedState::Readable);
            assert_eq!(read.unwrap(), content, "{name}");
        }
    }
    assert!(files.list("/Unterlagen/Belege").await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_untouched_folder_reads_whole() {
    let scene = filled().await;
    expect(&scene, &[], 0).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_byte_in_a_content_object_damages_only_that_file() {
    let scene = filled().await;
    let (content, _) = keys_of(&scene, "c.bin").await;
    edit(&scene, &content, |body| body[HEADER_LEN + 3] ^= 0x20);
    expect(&scene, &["c.bin"], 0).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_byte_in_a_sidecar_leaves_a_damaged_entry() {
    let scene = filled().await;
    let (_, sidecar) = keys_of(&scene, "a.txt").await;
    edit(&scene, &sidecar, |body| {
        let last = body.len() - 2;
        body[last] ^= 1;
    });
    expect(&scene, &["a.txt"], 1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_head_damages_the_folder() {
    let scene = filled().await;
    let head = scene
        .keys()
        .into_iter()
        .find(|key| key.ends_with(".hxef/h"))
        .unwrap();
    edit(&scene, &head, |body| {
        let last = body.len() - 1;
        body[last] ^= 1;
    });
    let files = scene.fresh_files();
    let root = files.list("/").await.unwrap();
    assert!(root
        .iter()
        .any(|entry| entry.encrypted.unwrap().state == EncryptedState::Damaged));
    let damaged = root
        .iter()
        .find(|entry| entry.encrypted.unwrap().state == EncryptedState::Damaged)
        .unwrap();
    let error = files.list(&damaged.path).await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Damaged);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_truncated_or_extended_content_object_is_damaged() {
    let scene = filled().await;
    let (c, _) = keys_of(&scene, "c.bin").await;
    edit(&scene, &c, |body| body.truncate(HEADER_LEN + CHUNK + 16));
    let (d, _) = keys_of(&scene, "d.bin").await;
    edit(&scene, &d, |body| body.extend_from_slice(&[0; 16]));
    expect(&scene, &["c.bin", "d.bin"], 0).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn swapped_content_objects_and_moved_blocks_are_damaged() {
    let scene = filled().await;
    let (a, _) = keys_of(&scene, "a.txt").await;
    let (b, _) = keys_of(&scene, "b.txt").await;
    let (body_a, body_b) = (
        scene.fake.object(BUCKET, &a).unwrap(),
        scene.fake.object(BUCKET, &b).unwrap(),
    );
    scene.fake.insert(BUCKET, &a, &body_b);
    scene.fake.insert(BUCKET, &b, &body_a);
    let (c, _) = keys_of(&scene, "c.bin").await;
    edit(&scene, &c, |body| {
        let block = CHUNK + 16;
        let (first, rest) = body[HEADER_LEN..].split_at_mut(block);
        first.swap_with_slice(&mut rest[..block]);
    });
    expect(&scene, &["a.txt", "b.txt", "c.bin"], 0).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sidecars_copied_under_another_sid_or_from_another_folder_are_damaged() {
    let scene = filled().await;
    let (_, sidecar) = keys_of(&scene, "a.txt").await;
    let body = scene.fake.object(BUCKET, &sidecar).unwrap();
    let (folder_m, _) = sidecar.rsplit_once('/').unwrap();
    scene.fake.insert(
        BUCKET,
        &format!("{folder_m}/{}", "b".repeat(25) + "a"),
        &body,
    );
    let other = scene
        .keys()
        .into_iter()
        .find(|key| key.contains(".hxef/m/") && !key.starts_with(folder_m))
        .unwrap();
    let foreign = scene.fake.object(BUCKET, &other).unwrap();
    let (_, foreign_sid) = other.rsplit_once('/').unwrap();
    scene
        .fake
        .insert(BUCKET, &format!("{folder_m}/{foreign_sid}"), &foreign);
    expect(&scene, &[], 2).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_old_content_object_put_back_is_damaged() {
    let scene = filled().await;
    let (old, _) = keys_of(&scene, "a.txt").await;
    let old_body = scene.fake.object(BUCKET, &old).unwrap();
    // A new version of a.txt with the same length, then the old object under the new name.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), &b"Rechnung Wasser"[..14]).unwrap();
    transfer(
        TransferOp::Copy,
        Side::Device,
        &[dir.path().join("a.txt").to_string_lossy().into_owned()],
        Side::Storage(Arc::new(scene.files())),
        "/Unterlagen",
        Some(crate::files::transfer::ConflictChoice::Replace),
        None,
    )
    .await
    .unwrap();
    let (new, _) = keys_of(&scene, "a.txt").await;
    assert_ne!(new, old);
    scene.fake.insert(BUCKET, &new, &old_body);
    let files = scene.fresh_files();
    let error = files
        .read_start("/Unterlagen/a.txt", 0, 100)
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Damaged);
    assert_eq!(
        files.read_start("/Unterlagen/b.txt", 0, 100).await.unwrap(),
        originals()["b.txt"]
    );
}

use super::*;
use crate::files::encrypted::format::folder_id;
use crate::files::encrypted::format::head::{open_head, HeadOutcome};
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::encrypted::test_support::{give_key, Scene, BUCKET};
use crate::files::EncryptedState;
use crate::remote_storage::test_support::vault;

#[test]
fn a_device_without_a_content_key_has_none() {
    let (_dir, db) = vault();
    assert!(db.read_blocking(|q| current(q)).expect("read").is_none());
    assert!(db
        .read_blocking(|q| HeldKeys::load(q))
        .expect("read")
        .is_empty());
    assert_eq!(no_key().code, crate::files::FilesErrorCode::NoKey);
}

#[test]
fn a_new_folder_takes_the_newest_generation_and_old_ones_still_open() {
    let (_dir, db) = vault();
    let first = give_key(&db, 1);
    let second = give_key(&db, 2);
    let newest = db
        .read_blocking(|q| current(q))
        .expect("read")
        .expect("a key");
    assert_eq!(newest.key_id, second.key_id);
    let held = db.read_blocking(|q| HeldKeys::load(q)).expect("read");
    assert_eq!(held.by_key_id(&first.key_id).as_deref(), Some(&*first.key));
    assert_eq!(
        held.by_key_id(&second.key_id).as_deref(),
        Some(&*second.key)
    );
    assert!(held.by_key_id(&[0; 16]).is_none());
}

/// The generation each encrypted folder's head is wrapped for, by the folder's name.
fn head_generations(scene: &Scene, held: &HeldKeys) -> Vec<(String, Id)> {
    let mut out: Vec<(String, Id)> = scene
        .keys()
        .into_iter()
        .filter_map(|key| {
            let folder = key.strip_suffix("/h")?;
            let fid = folder_id(folder.rsplit('/').next()?)?;
            let head = scene.fake.object(BUCKET, &key)?;
            match open_head(&head, &fid, |key_id| held.by_key_id(key_id)) {
                HeadOutcome::Readable(opened) => Some((opened.name, opened.key_id)),
                other => panic!("{key} does not open: {other:?}"),
            }
        })
        .collect();
    out.sort();
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_of_the_old_generation_opens_after_a_new_one_arrived() {
    let scene = Scene::without_key();
    let first = give_key(&scene.db, 1);
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Alt").await.unwrap();
    files.create_folder("/Alt", "Belege").await.unwrap();

    // Removing another device issues generation 2; this device receives it.
    let second = give_key(&scene.db, 2);
    create_encrypted_folder(&files, "/", "Neu").await.unwrap();

    let held = scene.db.read_blocking(|q| HeldKeys::load(q)).expect("read");
    assert_eq!(
        head_generations(&scene, &held),
        [
            ("Alt".to_owned(), first.key_id),
            ("Neu".to_owned(), second.key_id)
        ]
    );
    let other = scene.fresh_files();
    let listed = other.list("/").await.unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed
        .iter()
        .all(|entry| entry.encrypted.as_ref().unwrap().state == EncryptedState::Readable));
    let inside = other.list("/Alt").await.unwrap();
    assert_eq!(inside.len(), 1);
    assert_eq!(inside[0].name, "Belege");
}

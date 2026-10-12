use super::*;
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::encrypted::test_support::Scene;
use crate::remote_storage::test_support::Op;

/// The id of the folder `name` this session created below `path`, without listing (so nothing is
/// cached yet).
fn fid_of(files: &StorageFiles, path: &str, name: &str) -> Id {
    let scope = files.scope().unwrap();
    scope
        .folders
        .heads(&scope.storage_id, &folder_prefix(path).unwrap())
        .unwrap()
        .iter()
        .find(|head| head.display == name)
        .and_then(|head| head.fid)
        .unwrap()
}

#[tokio::test]
async fn the_readable_folders_are_found_below_ordinary_folders_but_not_in_extension_areas() {
    let scene = Scene::new();
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    files.create_folder("/", "Privat").await.unwrap();
    create_encrypted_folder(&files, "/Privat", "Steuer")
        .await
        .unwrap();
    // A head in an extension's area is no folder of the user (038 FR-010).
    let hidden = scene.fresh_files();
    let fake_head = format!("holzi-ext/v/e/{}.hxef/h", encode_id(&[7; 16]));
    hidden
        .store()
        .put(
            hidden.access(),
            &fake_head,
            b"x".to_vec(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(5),
        )
        .await
        .unwrap();

    let found = readable_folders(&scene.fresh_files()).await.unwrap();
    let names: Vec<(&str, &str)> = found
        .iter()
        .map(|at| (at.head.display.as_str(), at.root.as_str()))
        .collect();
    assert_eq!(
        names,
        [("Steuer", "/Privat/Steuer"), ("Unterlagen", "/Unterlagen")]
    );
    assert!(found.iter().all(|at| at.rest.is_empty()));
}

#[tokio::test]
async fn a_folder_is_found_by_its_id_from_the_cache_or_by_a_walk() {
    let scene = Scene::new();
    let files = scene.files();
    files.create_folder("/", "Privat").await.unwrap();
    create_encrypted_folder(&files, "/Privat", "Steuer")
        .await
        .unwrap();
    let fid = fid_of(&files, "/Privat", "Steuer");

    // Another device: nothing in memory, no cached head, so the storage is walked.
    let mark = scene.fake.calls().len();
    let walked = find(&scene.fresh_files(), &fid).await.unwrap();
    assert!(scene.fake.calls()[mark..]
        .iter()
        .any(|(op, key)| *op == Op::ListDir && key.is_empty()));
    assert_eq!(walked.root, "/Privat/Steuer");
    assert_eq!(walked.head.fid, Some(fid));

    // The walk cached the head: the next lookup lists only the folder's own level.
    let fresh = scene.fresh_files();
    let mark = scene.fake.calls().len();
    let cached = find(&fresh, &fid).await.unwrap();
    assert_eq!(cached.root, "/Privat/Steuer");
    let lists: Vec<_> = scene.fake.calls()[mark..]
        .iter()
        .filter(|(op, _)| *op == Op::ListDir)
        .map(|(_, key)| key.clone())
        .collect();
    assert_eq!(lists, ["Privat/"]);
}

#[tokio::test]
async fn an_unknown_id_is_not_found() {
    let scene = Scene::new();
    create_encrypted_folder(&scene.files(), "/", "Unterlagen")
        .await
        .unwrap();
    let Err(error) = find(&scene.fresh_files(), &[9; 16]).await else {
        panic!("no folder has this id");
    };
    assert_eq!(error.code, FilesErrorCode::NotFound);
}

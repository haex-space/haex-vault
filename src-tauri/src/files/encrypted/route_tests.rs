use super::*;
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::encrypted::test_support::{Scene, BUCKET};
use crate::remote_storage::test_support::access;

fn located(route: Route) -> Located {
    match route {
        Route::Encrypted(at) => at,
        Route::Plain => panic!("not through an encrypted folder"),
    }
}

fn raw_folder(scene: &Scene) -> String {
    let key = scene
        .keys()
        .into_iter()
        .find(|key| key.ends_with(".hxef/h"))
        .unwrap();
    raw_name(key.trim_end_matches("/h")).to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn paths_go_through_encrypted_folders_by_their_names() {
    let scene = Scene::new();
    scene.fake.insert(BUCKET, "Privat/", b"");
    let files = scene.files();
    create_encrypted_folder(&files, "/Privat", "Unterlagen")
        .await
        .unwrap();
    let at = located(route(&files, "/Privat/Unterlagen/Belege").await.unwrap());
    assert_eq!(at.root, "/Privat/Unterlagen");
    assert_eq!(at.rest, ["Belege"]);
    assert_eq!(at.head.display, "Unterlagen");
    assert!(matches!(
        route(&files, "/Privat").await.unwrap(),
        Route::Plain
    ));
    let raw = format!("/Privat/{}", raw_folder(&scene));
    let error = route(&files, &raw).await.err().unwrap();
    assert_eq!(error.code, FilesErrorCode::NotFound);

    // A device that never listed /Privat finds the way too.
    let fresh = scene.fresh_files();
    let at = located(route(&fresh, "/Privat/Unterlagen").await.unwrap());
    assert!(at.rest.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_scope_encrypted_prefixes_stay_closed() {
    let scene = Scene::new();
    create_encrypted_folder(&scene.files(), "/", "Unterlagen")
        .await
        .unwrap();
    let agent = StorageFiles::new(scene.fake.clone(), access(BUCKET));
    let root = agent.list("/").await.unwrap();
    assert_eq!(root.len(), 1);
    assert_eq!(root[0].name, raw_folder(&scene));
    assert_eq!(root[0].encrypted, None);
    let path = format!("/{}", raw_folder(&scene));
    for error in [
        agent.list(&path).await.unwrap_err(),
        agent.stat(&format!("{path}/h")).await.unwrap_err(),
    ] {
        assert_eq!(error.code, FilesErrorCode::InEncrypted);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn another_vaults_folder_shows_by_its_prefix_and_stays_closed() {
    let mine = Scene::new();
    create_encrypted_folder(&mine.files(), "/", "Unterlagen")
        .await
        .unwrap();
    let other = Scene {
        fake: mine.fake.clone(),
        ..Scene::new()
    };
    let files = other.files();
    let root = files.list("/").await.unwrap();
    assert_eq!(root.len(), 1);
    assert_eq!(root[0].name, raw_folder(&mine));
    assert_eq!(
        root[0].encrypted,
        Some(EncryptedInfo {
            role: EncryptedRole::Folder,
            state: EncryptedState::OtherVault
        })
    );
    assert!(root[0].modified_ms.is_some(), "the head's time shows");
    let error = files.list(&root[0].path).await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::OtherVault);
    let error = files.create_folder(&root[0].path, "x").await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::OtherVault);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_name_an_ordinary_entry_has_too_is_a_conflict_copy() {
    let scene = Scene::new();
    create_encrypted_folder(&scene.files(), "/", "Fotos")
        .await
        .unwrap();
    scene.fake.insert(BUCKET, "Fotos/a.jpg", b"a");
    let root = scene.fresh_files().list("/").await.unwrap();
    let encrypted = root.iter().find(|e| e.encrypted.is_some()).unwrap();
    assert!(encrypted.conflict);
    assert!(
        encrypted.name.starts_with("Fotos (Konflikt "),
        "{}",
        encrypted.name
    );
    let plain = root.iter().find(|e| e.encrypted.is_none()).unwrap();
    assert_eq!(plain.name, "Fotos");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_listed_levels_count_as_known_plain() {
    let scene = Scene::new();
    scene.fake.insert(BUCKET, "Fotos/2026/a.jpg", b"a");
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    assert!(!plain_by_memory(&scene.folders, "s1", "/Fotos/2026/a.jpg"));
    files.list("/Fotos").await.unwrap();
    assert!(plain_by_memory(&scene.folders, "s1", "/Fotos/2026/a.jpg"));
    assert!(!plain_by_memory(
        &scene.folders,
        "s1",
        "/Fotos/2026/x/a.jpg"
    ));
    assert!(!plain_by_memory(&scene.folders, "s1", "/Unterlagen/a.jpg"));
    scene.folders.clear();
    assert!(!plain_by_memory(&scene.folders, "s1", "/Fotos/a.jpg"));
}

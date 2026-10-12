use super::*;
use crate::files::encrypted::format::is_folder_prefix;
use crate::files::encrypted::test_support::{Scene, BUCKET};
use crate::files::{EncryptedInfo, EncryptedRole, EntryKind};

fn names(entries: &[Entry]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    names.sort();
    names
}

#[tokio::test(flavor = "multi_thread")]
async fn create_writes_one_head_and_nothing_else() {
    let scene = Scene::new();
    scene.fake.insert(BUCKET, "Privat/", b"");
    let files = scene.files();
    let entry = create_encrypted_folder(&files, "/Privat", "Unterlagen")
        .await
        .unwrap();
    assert_eq!(entry.name, "Unterlagen");
    assert_eq!(entry.path, "/Privat/Unterlagen");
    assert_eq!(entry.kind, EntryKind::Dir);
    assert_eq!(
        entry.encrypted,
        Some(EncryptedInfo {
            role: EncryptedRole::Folder,
            state: EncryptedState::Readable
        })
    );
    let keys = scene.keys();
    assert_eq!(keys.len(), 2, "{keys:?}");
    let head = keys.iter().find(|key| *key != "Privat/").unwrap();
    let (prefix, last) = head.rsplit_once('/').unwrap();
    assert_eq!(last, "h");
    let folder = prefix.strip_prefix("Privat/").unwrap();
    assert!(is_folder_prefix(folder), "{folder}");
    assert!(!head.contains("Unterlagen"));

    // Another device of the vault sees it by its name.
    let other = scene.fresh_files();
    let listed = other.list("/Privat").await.unwrap();
    assert_eq!(names(&listed), ["Unterlagen"]);
    assert_eq!(listed[0].encrypted, entry.encrypted);
    assert!(other.list("/Privat/Unterlagen").await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn create_is_refused_inside_extension_areas_and_encrypted_folders() {
    let scene = Scene::new();
    let files = scene.files();
    for area in ["/holzi-ext/v/e", "/holzi-ext-dev/v/e"] {
        let error = create_encrypted_folder(&files, area, "Geheim")
            .await
            .unwrap_err();
        assert_eq!(error.code, FilesErrorCode::InExtensionArea, "{area}");
    }
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    let error = create_encrypted_folder(&files, "/Unterlagen", "Innen")
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::InEncrypted);
    assert_eq!(scene.keys().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_refuses_a_name_taken_by_an_ordinary_or_an_encrypted_entry() {
    let scene = Scene::new();
    scene.fake.insert(BUCKET, "Fotos/a.jpg", b"a");
    scene.fake.insert(BUCKET, "notiz.txt", b"n");
    let files = scene.files();
    for taken in ["Fotos", "notiz.txt"] {
        let error = create_encrypted_folder(&files, "/", taken)
            .await
            .unwrap_err();
        assert_eq!(error.code, FilesErrorCode::Exists, "{taken}");
    }
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    let error = create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Exists);
    // An ordinary folder may not take the decrypted name either.
    let error = files.create_folder("/", "Unterlagen").await.unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Exists);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_without_a_content_key_is_refused() {
    let scene = Scene::without_key();
    let error = create_encrypted_folder(&scene.files(), "/", "Unterlagen")
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::NoKey);
    assert!(scene.keys().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sub_folder_is_one_sidecar_and_survives_a_fresh_load() {
    let scene = Scene::new();
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    let before = scene.keys();
    let entry = files.create_folder("/Unterlagen", "Belege").await.unwrap();
    assert_eq!(entry.path, "/Unterlagen/Belege");
    assert_eq!(entry.encrypted.unwrap().role, EncryptedRole::Inside);
    files
        .create_folder("/Unterlagen/Belege", "2026")
        .await
        .unwrap();
    let added: Vec<String> = scene
        .keys()
        .into_iter()
        .filter(|key| !before.contains(key))
        .collect();
    assert_eq!(added.len(), 2);
    assert!(
        added.iter().all(|key| key.contains(".hxef/m/")),
        "{added:?}"
    );
    let error = files
        .create_folder("/Unterlagen", "Belege")
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Exists);

    let other = scene.fresh_files();
    assert_eq!(names(&other.list("/Unterlagen").await.unwrap()), ["Belege"]);
    assert_eq!(
        names(&other.list("/Unterlagen/Belege").await.unwrap()),
        ["2026"]
    );
    let stat = other.stat("/Unterlagen/Belege/2026").await.unwrap();
    assert_eq!(stat.kind, EntryKind::Dir);
}

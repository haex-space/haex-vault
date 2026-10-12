use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::files::encrypted::format::is_folder_prefix;
use crate::files::encrypted::test_support::{transfer, Scene, BUCKET};
use crate::files::transfer::local::Outcome;
use crate::files::transfer::remote::Side;
use crate::files::transfer::{ConflictChoice, TransferOp};
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

/// Writes `content` to the device file `dir/name` and copies it into `/Unterlagen` of `files`.
async fn put(files: Arc<StorageFiles>, dir: &Path, name: &str, content: &str) {
    let file = dir.join(name);
    std::fs::write(&file, content).unwrap();
    let outcome = transfer(
        TransferOp::Copy,
        Side::Device,
        &[file.to_string_lossy().into_owned()],
        Side::Storage(files),
        "/Unterlagen",
        Some(ConflictChoice::Replace),
        None,
    )
    .await
    .unwrap();
    assert_eq!(outcome, Outcome::Done);
}

/// The entry named `name` and the one named as its conflict copy, as a third device lists them.
async fn kept_and_copy(scene: &Scene, stem: &str, ext: &str) -> (Entry, Entry) {
    let listed = scene.fresh_files().list("/Unterlagen").await.unwrap();
    assert_eq!(listed.len(), 2, "{:?}", names(&listed));
    let (copies, kept): (Vec<Entry>, Vec<Entry>) = listed
        .into_iter()
        .partition(|entry| entry.name.starts_with(&format!("{stem} (Konflikt ")));
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].name, format!("{stem}{ext}"));
    assert_eq!(copies.len(), 1);
    assert!(
        copies[0].name.ends_with(&format!("){ext}")),
        "{}",
        copies[0].name
    );
    (kept[0].clone(), copies[0].clone())
}

#[tokio::test(flavor = "multi_thread")]
async fn two_devices_replacing_one_file_keep_both_the_older_as_a_conflict_copy() {
    let scene = Scene::new();
    create_encrypted_folder(&scene.files(), "/", "Unterlagen")
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    put(Arc::new(scene.files()), dir.path(), "notiz.txt", "eins").await;
    // The second device has the folder loaded with the first version.
    let second = Arc::new(scene.fresh_files());
    assert_eq!(
        names(&second.list("/Unterlagen").await.unwrap()),
        ["notiz.txt"]
    );

    // Both replace that version, the second without seeing the first's.
    put(Arc::new(scene.files()), dir.path(), "notiz.txt", "von eins").await;
    put(second, dir.path(), "notiz.txt", "von zwei, laenger").await;

    let (kept, copy) = kept_and_copy(&scene, "notiz", ".txt").await;
    assert!(
        copy.modified_ms <= kept.modified_ms,
        "the older one is the copy"
    );
    let mut sizes = [kept.size, copy.size];
    sizes.sort();
    assert_eq!(sizes, [Some(8), Some(17)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_devices_adding_one_name_both_show_one_as_a_conflict_copy() {
    let scene = Scene::new();
    create_encrypted_folder(&scene.files(), "/", "Unterlagen")
        .await
        .unwrap();
    let second = Arc::new(scene.fresh_files());
    assert!(second.list("/Unterlagen").await.unwrap().is_empty());
    let dir = tempfile::tempdir().unwrap();
    put(Arc::new(scene.files()), dir.path(), "brief.txt", "eins").await;
    put(second, dir.path(), "brief.txt", "zwei!").await;

    // The source file modification time is not the conflict ordering key.  Two devices can
    // write the same name with filesystem timestamps that have the same resolution (or that
    // differ from the sidecar write order), while both versions must still remain visible.
    kept_and_copy(&scene, "brief", ".txt").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_encrypted_folders_of_one_name_both_show_one_as_a_conflict_copy() {
    let scene = Scene::new();
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    // Two devices that create the name at once both pass the check before either head is
    // written. The head does not carry its parent, so a head created elsewhere and put beside
    // the first is that outcome.
    files.create_folder("/", "Ablage").await.unwrap();
    create_encrypted_folder(&files, "/Ablage", "Unterlagen")
        .await
        .unwrap();
    let raced = scene
        .keys()
        .into_iter()
        .find(|key| key.starts_with("Ablage/") && key.ends_with(".hxef/h"))
        .unwrap();
    let head = scene.fake.object(BUCKET, &raced).unwrap();
    scene
        .fake
        .insert(BUCKET, raced.strip_prefix("Ablage/").unwrap(), &head);

    let other = scene.fresh_files();
    let listed = other.list("/").await.unwrap();
    let encrypted: Vec<&Entry> = listed
        .iter()
        .filter(|entry| entry.encrypted.is_some())
        .collect();
    assert_eq!(encrypted.len(), 2, "{:?}", names(&listed));
    assert!(encrypted.iter().any(|entry| entry.name == "Unterlagen"));
    let copy = encrypted
        .iter()
        .find(|entry| entry.name.starts_with("Unterlagen (Konflikt "))
        .expect("a conflict copy");
    assert!(other.list(&copy.path).await.unwrap().is_empty());
}

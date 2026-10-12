//! "Mit System-App öffnen" for files of encrypted folders (T048): the notice, "Nicht mehr fragen"
//! per folder, and copies only in their own place.

use super::*;
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::test_support::{filled, tree, BUCKET};

async fn open_copy(
    files: &StorageFiles,
    path: &str,
    root: &Path,
    confirmed: bool,
    dont_ask_again: bool,
) -> Result<PathBuf, FilesError> {
    let Route::Encrypted(at) = route(files, path).await.unwrap() else {
        panic!("{path} is in an encrypted folder");
    };
    copy_for_app(files, &at, root, confirmed, dont_ask_again).await
}

fn copies(root: &Path) -> usize {
    std::fs::read_dir(root).map_or(0, |dirs| dirs.count())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_needs_the_notice_until_dont_ask_again() {
    let scene = filled(&tree(&[("notiz.txt", "Strom 2026")])).await;
    let cache = tempfile::tempdir().unwrap();
    let root = cache.path().join(FOLDER);
    let files = scene.files();
    let path = "/Unterlagen/notiz.txt";

    let error = open_copy(&files, path, &root, false, false)
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::NeedsNotice);
    assert!(!error.message.contains("notiz"));
    assert_eq!(copies(&root), 0, "nothing decrypted before the notice");

    let copy = open_copy(&files, path, &root, true, false).await.unwrap();
    assert_eq!(std::fs::read(&copy).unwrap(), b"Strom 2026");
    assert_eq!(copy.file_name().unwrap(), "notiz.txt");
    assert!(copy.starts_with(&root));
    let error = open_copy(&files, path, &root, false, false)
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::NeedsNotice, "asked again");

    open_copy(&files, path, &root, true, true).await.unwrap();
    open_copy(&files, path, &root, false, false).await.unwrap();
    assert_eq!(copies(&root), 3, "a folder per copy");

    // The answer stays with the folder: after a rename, and on the next start.
    files.rename("/Unterlagen", "Akten").await.unwrap();
    let later = scene.fresh_files();
    open_copy(&later, "/Akten/notiz.txt", &root, false, false)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn dont_ask_again_holds_for_its_folder_only() {
    let scene = filled(&tree(&[("notiz.txt", "a")])).await;
    crate::files::encrypted::ops::create_encrypted_folder(&scene.files(), "/", "Andere")
        .await
        .unwrap();
    let device = tempfile::tempdir().unwrap();
    std::fs::write(device.path().join("b.txt"), "b").unwrap();
    crate::files::encrypted::test_support::transfer(
        crate::files::transfer::TransferOp::Copy,
        crate::files::transfer::remote::Side::Device,
        &[device.path().join("b.txt").to_string_lossy().into_owned()],
        crate::files::transfer::remote::Side::Storage(std::sync::Arc::new(scene.files())),
        "/Andere",
        None,
        None,
    )
    .await
    .unwrap();
    let cache = tempfile::tempdir().unwrap();
    let files = scene.files();
    open_copy(&files, "/Unterlagen/notiz.txt", cache.path(), true, true)
        .await
        .unwrap();
    let error = open_copy(&files, "/Andere/b.txt", cache.path(), false, false)
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::NeedsNotice);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_damaged_file_leaves_no_copy() {
    let scene = filled(&tree(&[("notiz.txt", "Strom 2026")])).await;
    let content = scene
        .keys()
        .into_iter()
        .find(|key| key.contains(".hxef/c/"))
        .unwrap();
    let mut body = scene.fake.object(BUCKET, &content).unwrap();
    let last = body.len() - 1;
    body[last] ^= 1;
    scene.fake.insert(BUCKET, &content, &body);
    let cache = tempfile::tempdir().unwrap();
    let error = open_copy(
        &scene.fresh_files(),
        "/Unterlagen/notiz.txt",
        cache.path(),
        true,
        false,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Damaged);
    assert_eq!(copies(cache.path()), 0);
}

#[test]
fn the_sweep_removes_every_copy() {
    let cache = tempfile::tempdir().unwrap();
    let root = cache.path().join(FOLDER);
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::write(root.join("a/brief.pdf"), "x").unwrap();
    sweep(&root);
    assert!(!root.exists());
    sweep(&root);
}

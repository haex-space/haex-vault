use tokio::io::AsyncReadExt;

use super::*;
use crate::files::encrypted::format::content::HEADER_LEN;
use crate::files::encrypted::format::CHUNK;
use crate::files::encrypted::ops::{create_encrypted_folder, file_at};
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::test_support::{transfer, Scene, BUCKET};
use crate::files::transfer::remote::Side;
use crate::files::transfer::TransferOp;
use crate::files::{EncryptedState, FilesErrorCode};
use crate::remote_storage::test_support::Op;

/// A folder "Unterlagen" with `daten.bin` of `content`.
async fn scene_with(content: &[u8]) -> Scene {
    let scene = Scene::new();
    create_encrypted_folder(&scene.files(), "/", "Unterlagen")
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daten.bin");
    std::fs::write(&path, content).unwrap();
    transfer(
        TransferOp::Copy,
        Side::Device,
        &[path.to_string_lossy().into_owned()],
        Side::Storage(Arc::new(scene.files())),
        "/Unterlagen",
        None,
        None,
    )
    .await
    .unwrap();
    scene
}

async fn opened(files: &StorageFiles) -> EncryptedFile {
    let Route::Encrypted(at) = route(files, "/Unterlagen/daten.bin").await.unwrap() else {
        panic!("encrypted");
    };
    let (folder, version) = file_at(files, &at).await.unwrap();
    EncryptedFile::open(files, folder, &version).await.unwrap()
}

fn content(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 253) as u8).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_fetches_only_its_blocks() {
    let data = content(3 * CHUNK + 100);
    let scene = scene_with(&data).await;
    let files = scene.fresh_files();
    let file = opened(&files).await;
    assert_eq!(file.size(), data.len() as u64);
    let before = scene.fake.calls().len();
    let start = CHUNK as u64 + 10;
    let end = 2 * CHUNK as u64 + 20;
    assert_eq!(
        file.read(start, end).await.unwrap(),
        &data[start as usize..end as usize]
    );
    let calls = &scene.fake.calls()[before..];
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].0, Op::GetRange);

    let mut all = Vec::new();
    Arc::new(file)
        .reader(5, data.len() as u64)
        .read_to_end(&mut all)
        .await
        .unwrap();
    assert_eq!(all, &data[5..]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_block_is_refused_and_marks_the_entry_damaged() {
    let data = content(2 * CHUNK);
    let scene = scene_with(&data).await;
    let key = scene
        .keys()
        .into_iter()
        .find(|key| key.contains(".hxef/c/"))
        .unwrap();
    let mut object = scene.fake.object(BUCKET, &key).unwrap();
    object[HEADER_LEN + CHUNK + 16 + 5] ^= 1;
    scene.fake.insert(BUCKET, &key, &object);

    let files = scene.fresh_files();
    let file = opened(&files).await;
    assert_eq!(
        file.read(0, 10).await.unwrap(),
        &data[..10],
        "block 0 is fine"
    );
    let error = file
        .read(CHUNK as u64, CHUNK as u64 + 10)
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Damaged);
    let listed = files.list("/Unterlagen").await.unwrap();
    assert_eq!(listed[0].encrypted.unwrap().state, EncryptedState::Damaged);
    let error = files
        .read_start("/Unterlagen/daten.bin", 0, 100)
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Damaged);
}

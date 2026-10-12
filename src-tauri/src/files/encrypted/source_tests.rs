use tokio::io::{AsyncRead, AsyncReadExt};

use super::*;
use crate::files::encrypted::format::content::HEADER_LEN;
use crate::files::encrypted::format::{CHUNK, TAG_LEN};
use crate::files::encrypted::ops::{create_encrypted_folder, file_at};
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::test_support::{transfer, Scene, BUCKET};
use crate::files::media::server::CHUNK as SERVER_CHUNK;
use crate::files::transfer::remote::Side;
use crate::files::transfer::TransferOp;
use crate::files::{EncryptedState, FilesErrorCode};
use crate::remote_storage::test_support::Op;

/// One block in the content object.
const BLOCK: u64 = CHUNK as u64 + TAG_LEN as u64;

/// A file larger than two windows that ends in a partial block: windows of blocks 0–127,
/// 128–255 and 256–257.
const BIG: usize = 2 * WINDOW as usize + CHUNK + 100;

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

/// The `(start, len)` of the requests to the content object since the first `before` of all.
fn fetched(scene: &Scene, before: usize) -> Vec<(u64, u64)> {
    scene.fake.ranges()[before..]
        .iter()
        .map(|(_, start, len)| (*start, *len))
        .collect()
}

/// The request for blocks `first..=last` of a file of `size` bytes.
fn blocks(first: u64, last: u64, size: u64) -> (u64, u64) {
    let header = HEADER_LEN as u64;
    let object = header + size + size.div_ceil(CHUNK as u64) * TAG_LEN as u64;
    let start = header + first * BLOCK;
    let end = (header + (last + 1) * BLOCK).min(object);
    (start, end - start)
}

/// Reads `reader` as the media server does, through its buffer, and calls `each` after every
/// read; the bytes and the error that ended the stream, if one did.
async fn drain(
    mut reader: Box<dyn AsyncRead + Send + Unpin>,
    mut each: impl FnMut(),
) -> (Vec<u8>, Option<std::io::Error>) {
    let mut all = Vec::new();
    let mut buf = vec![0u8; SERVER_CHUNK];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => return (all, None),
            Ok(read) => all.extend_from_slice(&buf[..read]),
            Err(error) => return (all, Some(error)),
        }
        each();
    }
}

async fn range_of(source: &EncryptedFileSource, start: u64, len: u64) -> Vec<u8> {
    let (bytes, error) = drain(source.open_range(start, len).await.unwrap(), || {}).await;
    assert!(error.is_none(), "{error:?}");
    bytes
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
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_in_the_middle_fetches_its_blocks_once_and_reading_on_reuses_them() {
    let data = content(3 * CHUNK + 100);
    let size = data.len() as u64;
    let scene = scene_with(&data).await;
    let files = scene.fresh_files();
    let source = EncryptedFileSource::new(opened(&files).await, "video/mp4");
    assert_eq!(source.size().await.unwrap(), size);
    assert_eq!(source.content_type(), "video/mp4");
    let before = scene.fake.ranges().len();

    let start = CHUNK as u64 + 10;
    let len = CHUNK as u64 + 10;
    let middle = range_of(&source, start, len).await;
    assert_eq!(middle, &data[start as usize..(start + len) as usize]);
    assert_eq!(fetched(&scene, before), vec![blocks(1, 2, size)]);

    // The same place again and on to the end of block 2: no new request.
    assert_eq!(range_of(&source, start, len).await, middle);
    let on = range_of(&source, start + len, 3 * CHUNK as u64 - start - len).await;
    assert_eq!(on, &data[(start + len) as usize..3 * CHUNK]);
    assert_eq!(fetched(&scene, before).len(), 1);

    // Past the end it stops at the last, partial block.
    let tail = range_of(&source, size - 50, 1000).await;
    assert_eq!(tail, &data[data.len() - 50..]);
    assert_eq!(fetched(&scene, before)[1], blocks(3, 3, size));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_large_file_streams_window_by_window_with_bounded_memory() {
    let data = content(BIG);
    let size = data.len() as u64;
    let scene = scene_with(&data).await;
    let files = scene.fresh_files();
    let source = EncryptedFileSource::new(opened(&files).await, "video/mp4");
    let before = scene.fake.ranges().len();

    let mut most = 0;
    let (all, error) = drain(source.open_range(0, size).await.unwrap(), || {
        most = most.max(source.kept());
    })
    .await;
    assert!(error.is_none(), "{error:?}");
    assert!(all == data, "the whole file streams back unchanged");
    // One request per window, never more than a window of plaintext; at most two windows kept,
    // whatever the size of the file.
    assert_eq!(
        fetched(&scene, before),
        vec![
            blocks(0, 127, size),
            blocks(128, 255, size),
            blocks(256, 257, size)
        ]
    );
    assert!(most <= KEPT * WINDOW as usize, "{most}");
    assert_eq!(source.kept(), WINDOW as usize + CHUNK + 100);

    // Across the border of the first two windows: the first was dropped, so blocks 127 and 128
    // come in one request.
    let at = WINDOW - 10;
    assert_eq!(
        range_of(&source, at, 20).await,
        &data[at as usize..at as usize + 20]
    );
    assert_eq!(fetched(&scene, before)[3], blocks(127, 128, size));
    // The last window is still kept.
    assert_eq!(
        range_of(&source, size - 50, 50).await,
        &data[data.len() - 50..]
    );
    assert_eq!(fetched(&scene, before).len(), 4);
    assert!(source.kept() <= KEPT * WINDOW as usize);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_block_mid_stream_ends_the_stream_before_any_byte_of_it() {
    let data = content(BIG);
    let scene = scene_with(&data).await;
    let key = scene
        .keys()
        .into_iter()
        .find(|key| key.contains(".hxef/c/"))
        .unwrap();
    let mut object = scene.fake.object(BUCKET, &key).unwrap();
    object[HEADER_LEN + 130 * BLOCK as usize + 7] ^= 1;
    scene.fake.insert(BUCKET, &key, &object);

    let files = scene.fresh_files();
    let source = EncryptedFileSource::new(opened(&files).await, "video/mp4");
    let (got, error) = drain(
        source.open_range(0, data.len() as u64).await.unwrap(),
        || {},
    )
    .await;
    assert!(error.is_some(), "the stream ends with an error");
    assert_eq!(got.len(), WINDOW as usize, "the first window only");
    assert!(got == data[..WINDOW as usize], "never changed plaintext");
    let listed = files.list("/Unterlagen").await.unwrap();
    assert_eq!(listed[0].encrypted.unwrap().state, EncryptedState::Damaged);
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

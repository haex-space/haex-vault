use std::io::Cursor;

use super::*;
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::encrypted::test_support::{transfer, Scene};
use crate::files::local::OwnPlaces;
use crate::files::transfer::remote::Side;
use crate::files::transfer::TransferOp;

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbImage::from_pixel(40, 30, image::Rgb([200, 10, 10]))
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes
}

#[tokio::test(flavor = "multi_thread")]
async fn an_encrypted_image_gets_its_thumbnail_in_memory_only() {
    let scene = Scene::new();
    create_encrypted_folder(&scene.files(), "/", "Unterlagen")
        .await
        .unwrap();
    let device = tempfile::tempdir().unwrap();
    let image = device.path().join("bild.png");
    let bytes = png();
    std::fs::write(&image, &bytes).unwrap();
    transfer(
        TransferOp::Copy,
        Side::Device,
        &[image.to_string_lossy().into_owned()],
        Side::Storage(Arc::new(scene.files())),
        "/Unterlagen",
        None,
        None,
    )
    .await
    .unwrap();

    let cache = tempfile::tempdir().unwrap();
    let files = FilesState::new(OwnPlaces::default(), Vec::new(), cache.path().join("t"));
    let path = "/Unterlagen/bild.png".to_owned();
    let size = bytes.len() as u64;
    let open = || async { Ok(scene.files()) };
    let made = storage_thumbnail(&files, "s1", path.clone(), size, 1, open)
        .await
        .unwrap();
    assert_eq!(&made[..2], [0xff, 0xd8], "a JPEG");
    assert!(files.memory_thumbnails.size() > 0);
    let again = storage_thumbnail(&files, "s1", path, size, 1, || async {
        unreachable!("the second answer comes from memory")
    })
    .await
    .unwrap();
    assert_eq!(again, made);
    let written: Vec<_> = std::fs::read_dir(cache.path()).unwrap().collect();
    assert!(written.is_empty(), "nothing under the cache: {written:?}");
}

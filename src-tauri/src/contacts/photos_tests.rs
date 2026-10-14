use std::io::Cursor;

use image::{DynamicImage, ImageFormat, RgbImage};

use super::*;
use crate::passwords::test_support::open_test_vault;
use crate::storage::query::read;

/// A PNG with a busy pattern, so its JPEG does not come out small by itself.
fn busy_png(width: u32, height: u32) -> Vec<u8> {
    let image = RgbImage::from_fn(width, height, |x, y| {
        let v = x.wrapping_mul(7919) ^ y.wrapping_mul(104_729) ^ (x * y);
        image::Rgb([
            (v & 0xff) as u8,
            ((v >> 8) & 0xff) as u8,
            ((v >> 16) & 0xff) as u8,
        ])
    });
    let mut png = Vec::new();
    DynamicImage::ImageRgb8(image)
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .expect("encode png");
    png
}

fn dimensions(jpeg: &[u8]) -> (u32, u32) {
    let image = image::load_from_memory(jpeg).expect("a jpeg");
    (image.width(), image.height())
}

#[test]
fn a_large_photo_becomes_a_small_jpeg() {
    let jpeg = shrink(&busy_png(3000, 2000)).expect("shrink");
    let (width, height) = dimensions(&jpeg);
    assert!(width <= EDGE && height <= EDGE, "{width} × {height}");
    assert_eq!(width, EDGE, "the longest edge is used");
    assert!(jpeg.len() <= MAX_BYTES, "{} bytes", jpeg.len());
    assert!(jpeg.starts_with(&[0xff, 0xd8]), "a JPEG");
}

#[test]
fn a_small_photo_is_never_enlarged() {
    let jpeg = shrink(&busy_png(100, 80)).expect("shrink");
    assert_eq!(dimensions(&jpeg), (100, 80));
}

#[test]
fn something_that_is_no_image_is_refused() {
    assert!(matches!(
        shrink(b"not an image at all"),
        Err(HolziError::ContactsInvalid { .. })
    ));
}

fn binaries(db: &haex_crdt::Database) -> Vec<(String, Option<String>)> {
    read(db, |q| {
        q.query_map(
            "SELECT hash, orphaned_at FROM haex_contacts_binaries ORDER BY hash",
            &[],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    })
    .expect("read")
}

#[test]
fn the_same_photo_is_stored_once() {
    let (_dir, db) = open_test_vault();
    let jpeg = shrink(&busy_png(64, 64)).expect("shrink");
    let first = db.write(|tx| Ok(store(tx, &jpeg)?)).expect("store");
    let second = db.write(|tx| Ok(store(tx, &jpeg)?)).expect("store");
    assert_eq!(first, second);
    assert_eq!(binaries(&db).len(), 1);
    let stored = read(&db, |q| Ok(data(q, &first)?)).expect("data");
    assert_eq!(stored, jpeg);
}

#[test]
fn an_unused_photo_is_marked_and_deleted_after_seven_days() {
    let (_dir, db) = open_test_vault();
    let jpeg = shrink(&busy_png(64, 64)).expect("shrink");
    let hash = db.write(|tx| Ok(store(tx, &jpeg)?)).expect("store");
    let day = 86_400_000;
    let now = clock::unix_millis(std::time::SystemTime::now());

    assert_eq!(db.write(move |tx| Ok(prune(tx, now)?)).expect("prune"), 0);
    assert!(binaries(&db)[0].1.is_some(), "marked at the first look");

    let later = now + 6 * day;
    assert_eq!(db.write(move |tx| Ok(prune(tx, later)?)).expect("prune"), 0);
    assert_eq!(binaries(&db).len(), 1, "kept within the grace period");

    let expired = now + 7 * day + 1000;
    assert_eq!(
        db.write(move |tx| Ok(prune(tx, expired)?)).expect("prune"),
        1
    );
    assert!(binaries(&db).is_empty());
    let _ = hash;
}

#[test]
fn a_photo_in_use_keeps_or_loses_its_mark() {
    let (_dir, db) = open_test_vault();
    let jpeg = shrink(&busy_png(64, 64)).expect("shrink");
    let hash = db.write(|tx| Ok(store(tx, &jpeg)?)).expect("store");
    let now = clock::unix_millis(std::time::SystemTime::now());
    db.write(move |tx| Ok(prune(tx, now)?)).expect("prune");
    assert!(binaries(&db)[0].1.is_some());

    let used = hash.clone();
    db.write(move |tx| {
        tx.execute(
            "INSERT INTO haex_contacts (id, given_name, photo, origin_kind, origin_device, \
             origin_caller, created_at, updated_at) \
             VALUES ('c1', 'Anna', ?1, 'holzi', 'd', 'user', 't', 't')",
            params![used],
        )?;
        Ok(())
    })
    .expect("a contact uses the photo");
    db.write(move |tx| Ok(prune(tx, now)?)).expect("prune");
    assert!(binaries(&db)[0].1.is_none(), "used again, so no mark");

    db.write(move |tx| {
        tx.execute("UPDATE haex_contacts SET photo = NULL WHERE id = 'c1'", &[])?;
        Ok(mark_if_unused(tx, &hash)?)
    })
    .expect("drop the photo");
    assert!(binaries(&db)[0].1.is_some(), "unused, so marked");
}

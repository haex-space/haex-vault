//! Images for models (spec 044 FR-035, research R2): upright, the longest edge at most 1568 px, a
//! JPEG of at most 5 MB, and bytes that are no image refused.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, RgbImage};

use super::image::{encode_within, for_model, IMAGE_BYTES, IMAGE_EDGE};
use crate::files::thumbnails::tests::jpeg_with_orientation;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    DynamicImage::ImageRgb8(RgbImage::from_pixel(
        width,
        height,
        image::Rgb([10, 120, 200]),
    ))
    .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
    .expect("png");
    bytes
}

fn dims(jpeg: &[u8]) -> (u32, u32) {
    let image = image::load_from_memory_with_format(jpeg, ImageFormat::Jpeg).expect("a JPEG");
    (image.width(), image.height())
}

#[test]
fn a_large_image_shrinks_to_the_edge_as_a_jpeg() {
    let jpeg = for_model(&png(4000, 1000)).expect("image");
    assert_eq!(dims(&jpeg), (IMAGE_EDGE, IMAGE_EDGE / 4));
    assert!(jpeg.len() <= IMAGE_BYTES);
}

#[test]
fn a_small_image_keeps_its_size() {
    assert_eq!(dims(&for_model(&png(300, 200)).expect("image")), (300, 200));
}

#[test]
fn the_exif_orientation_turns_the_image_upright() {
    // Orientation 6: stored landscape, shown portrait.
    let jpeg = for_model(&jpeg_with_orientation(40, 20, 6)).expect("image");
    assert_eq!(dims(&jpeg), (20, 40));
}

#[test]
fn the_quality_drops_until_the_image_fits() {
    // Noise compresses badly, so every quality gives another size.
    let noise = RgbImage::from_fn(400, 400, |x, y| {
        let v = (x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)).wrapping_mul(2_654_435_761);
        image::Rgb([(v >> 8) as u8, (v >> 16) as u8, (v >> 24) as u8])
    });
    let full = encode_within(&noise, usize::MAX).expect("encoded");
    let limit = full.len() - 1;
    let smaller = encode_within(&noise, limit).expect("fits at a lower quality");
    assert!(smaller.len() <= limit);
    assert!(encode_within(&noise, 100).is_none(), "nothing that small");
}

#[test]
fn bytes_that_are_no_image_are_refused() {
    assert!(for_model(b"kein Bild").is_err());
}

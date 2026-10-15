use std::io::Cursor;

use image::{ImageFormat, RgbaImage};

use super::*;

fn encode(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
    let image = RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]));
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, format).expect("encode");
    out.into_inner()
}

#[test]
fn a_small_png_and_a_small_webp_pass() {
    assert_eq!(
        check_raster("a.png", &encode(16, 16, ImageFormat::Png)),
        Ok(RasterKind::Png)
    );
    assert_eq!(
        check_raster("A.WEBP", &encode(16, 16, ImageFormat::WebP)),
        Ok(RasterKind::Webp)
    );
}

#[test]
fn an_image_over_512_pixels_is_refused() {
    for (width, height) in [(513, 16), (16, 513)] {
        assert_eq!(
            check_raster("a.png", &encode(width, height, ImageFormat::Png)),
            Err(RasterError::TooLarge)
        );
    }
    assert!(check_raster("a.png", &encode(512, 512, ImageFormat::Png)).is_ok());
}

#[test]
fn a_file_that_is_not_what_its_name_says_is_refused() {
    let png = encode(8, 8, ImageFormat::Png);
    assert_eq!(check_raster("a.webp", &png), Err(RasterError::Mismatch));
    let webp = encode(8, 8, ImageFormat::WebP);
    assert_eq!(check_raster("a.png", &webp), Err(RasterError::Mismatch));
}

#[test]
fn a_corrupt_image_is_refused() {
    let mut png = encode(8, 8, ImageFormat::Png);
    png.truncate(png.len() / 2);
    assert_eq!(check_raster("a.png", &png), Err(RasterError::Unreadable));
    assert!(check_raster("a.png", b"not an image").is_err());
    assert!(check_raster("a.png", b"").is_err());
}

#[test]
fn other_formats_are_refused_by_name() {
    for name in ["a.gif", "a.jpg", "a.bmp", "a.svg", "a"] {
        assert_eq!(
            check_raster(name, &encode(8, 8, ImageFormat::Png)),
            Err(RasterError::Format),
            "{name}"
        );
    }
}

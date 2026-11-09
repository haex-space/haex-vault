use super::*;
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, GrayImage, Luma};

/// A JPEG of the QR code of `text`, as a camera would hand it over: black modules on white, a
/// quiet zone around, `scale` pixels per module.
fn jpeg_of(text: &str, scale: u32) -> Vec<u8> {
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M)
        .expect("encodes");
    let modules = code.width() as u32;
    let quiet = 4;
    let side = (modules + 2 * quiet) * scale;
    let mut picture = GrayImage::from_pixel(side, side, Luma([255]));
    for (index, color) in code.to_colors().into_iter().enumerate() {
        if color != qrcode::Color::Dark {
            continue;
        }
        let (x, y) = (
            (index as u32 % modules) + quiet,
            (index as u32 / modules) + quiet,
        );
        for dy in 0..scale {
            for dx in 0..scale {
                picture.put_pixel(x * scale + dx, y * scale + dy, Luma([0]));
            }
        }
    }
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 80)
        .encode(picture.as_raw(), side, side, ExtendedColorType::L8)
        .expect("encodes");
    jpeg
}

const LINK: &str = "haex-vault://link?v=1&c=ABCDEFGHIJKLMNOPQRSTUVWXYA&n=Privat&nr=wss%3A%2F%2Frelay.example.org&off=wss%3A%2F%2Fnos.lol";

#[test]
fn a_usual_link_text_is_read_from_a_jpeg() {
    assert_eq!(read_qr(&jpeg_of(LINK, 5)).as_deref(), Some(LINK));
}

#[test]
fn a_code_at_the_size_limit_is_read_from_a_jpeg() {
    let text = format!("{LINK}&off={}", "x".repeat(1_200 - LINK.len() - 5));

    assert_eq!(text.len(), 1_200);
    assert_eq!(read_qr(&jpeg_of(&text, 4)).as_deref(), Some(text.as_str()));
}

#[test]
fn a_picture_without_a_code_gives_none() {
    let mut blank = Vec::new();
    JpegEncoder::new(&mut blank)
        .encode(&[200u8; 320 * 240], 320, 240, ExtendedColorType::L8)
        .expect("encodes");

    assert_eq!(read_qr(&blank), None);
}

#[test]
fn bytes_that_are_no_jpeg_give_none() {
    assert_eq!(read_qr(b""), None);
    assert_eq!(read_qr(b"not a picture"), None);
    // A frame torn off in the middle.
    let jpeg = jpeg_of(LINK, 5);
    assert_eq!(read_qr(&jpeg[..jpeg.len() / 3]), None);
}

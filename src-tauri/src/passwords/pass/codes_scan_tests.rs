//! Reading codes from an image for a pass (spec 092, T072, FR-034): a photo with two codes gives
//! both, an image without a code gives none and no error, a value never prints, and a PDF is
//! told from an image.

use super::codes::{draw, CodeFormat};
use super::codes_tests::{png, raster};
use super::scan::{is_pdf, scan_image, ScannedCode};

fn sheet_with(values: &[(CodeFormat, &str)]) -> Vec<u8> {
    let mut rasters = Vec::new();
    for (format, value) in values {
        let drawing = draw(*format, value).expect("draw");
        rasters.push(raster(&drawing, 4));
    }
    let width: u32 = rasters.iter().map(|(_, w, _)| w + 40).sum();
    let height: u32 = rasters.iter().map(|(_, _, h)| *h).max().unwrap_or(0);
    let mut sheet = vec![255u8; (width * height) as usize];
    let mut left = 0;
    for (px, w, h) in &rasters {
        for y in 0..*h {
            for x in 0..*w {
                sheet[(y * width + left + x) as usize] = px[(y * w + x) as usize];
            }
        }
        left += w + 40;
    }
    png(&sheet, width, height)
}

#[test]
fn a_photo_with_two_codes_gives_both_with_their_formats() {
    let image = sheet_with(&[
        (CodeFormat::Qr, "SCAN-FIRST-1"),
        (CodeFormat::Aztec, "SCAN-SECOND-2"),
    ]);
    let found = scan_image(&image);
    let pairs: Vec<(&str, &str)> = found
        .iter()
        .map(|c| (c.format.as_str(), c.value.as_str()))
        .collect();
    assert!(pairs.contains(&("qr", "SCAN-FIRST-1")), "{found:?}");
    assert!(pairs.contains(&("aztec", "SCAN-SECOND-2")), "{found:?}");
}

#[test]
fn a_blank_image_and_garbage_give_none_without_an_error() {
    let blank = png(&vec![255u8; 64 * 64], 64, 64);
    assert!(scan_image(&blank).is_empty());
    assert!(scan_image(b"not an image at all").is_empty());
    assert!(scan_image(&[]).is_empty());
}

#[test]
fn a_scanned_code_never_prints_its_value() {
    let code = ScannedCode {
        format: "qr".to_owned(),
        value: "SECRET-MARKER-scan".to_owned(),
    };
    assert!(!format!("{code:?}").contains("SECRET-MARKER-scan"));
}

#[test]
fn a_pdf_is_told_from_an_image() {
    assert!(is_pdf(b"%PDF-1.7\n1 0 obj"));
    assert!(is_pdf(b"\n\n  %PDF-1.4"));
    assert!(!is_pdf(&png(&vec![255u8; 16 * 16], 16, 16)));
    assert!(!is_pdf(b""));
}

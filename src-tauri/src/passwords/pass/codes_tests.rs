//! Tests of the code module (spec 092, T024, SC-002): every format is checked, drawn as an SVG,
//! rasterised and read back to the stored value, and the drawing is dark on white with the quiet
//! zone of its format.

use super::codes::{check, decode, draw, CodeFormat, Drawing, Found, Reason};

/// One value of each format that its rules accept.
const SAMPLES: [(CodeFormat, &str); 5] = [
    (CodeFormat::Qr, "https://example.org/t?id=ÄÖ-123&seat=14C"),
    (CodeFormat::Code128, "ABC-1234567890"),
    (CodeFormat::Ean13, "4006381333931"),
    (
        CodeFormat::Pdf417,
        "TICKET|2026-11-14|ICE 1581|Berlin Hbf > Muenchen Hbf|Platz 42",
    ),
    (CodeFormat::Aztec, "AZTEC-VALUE-0042"),
];

/// The pixels of a drawing: the SVG rects of the symbol painted black on a white ground, `scale`
/// pixels to the module. Reads only what `draw` writes, so a change of its output shows here.
fn raster(drawing: &Drawing, scale: u32) -> (Vec<u8>, u32, u32) {
    let (w, h) = (drawing.width * scale, drawing.height * scale);
    let mut px = vec![255u8; (w * h) as usize];
    let number = |rect: &str, name: &str| -> Option<u32> {
        let key = format!(" {name}=\"");
        let start = rect.find(&key)? + key.len();
        let end = start + rect[start..].find('"')?;
        rect[start..end].parse().ok()
    };
    let group = drawing
        .svg
        .split_once("<g fill=\"#000\">")
        .expect("the symbol is one group of dark rects")
        .1;
    for rect in group.split("<rect").skip(1) {
        let (x, y) = (number(rect, "x").unwrap(), number(rect, "y").unwrap());
        let (rw, rh) = (
            number(rect, "width").unwrap(),
            number(rect, "height").unwrap(),
        );
        for py in y * scale..(y + rh) * scale {
            for px_x in x * scale..(x + rw) * scale {
                px[(py * w + px_x) as usize] = 0;
            }
        }
    }
    (px, w, h)
}

fn png(px: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(w, h, px.to_vec()).unwrap())
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

#[test]
fn every_format_is_checked_drawn_rasterised_and_read_back() {
    for (format, value) in SAMPLES {
        check(format, value).unwrap_or_else(|r| panic!("{format:?} refused its own sample: {r}"));
        let drawing = draw(format, value).expect("draw");
        let (px, w, h) = raster(&drawing, 4);
        let found = decode(&png(&px, w, h));
        assert_eq!(
            found,
            vec![Found {
                format,
                value: value.to_owned()
            }],
            "{format:?} did not read back"
        );
    }
}

#[test]
fn a_drawing_is_dark_on_white_with_the_quiet_zone_of_its_format() {
    for (format, value) in SAMPLES {
        let drawing = draw(format, value).expect("draw");
        assert!(
            drawing.svg.contains(
                "<rect width=\"%W\" height=\"%H\" fill=\"#fff\"/>"
                    .replace("%W", &drawing.width.to_string())
                    .replace("%H", &drawing.height.to_string())
                    .as_str()
            ),
            "{format:?}: a white ground over the whole view box"
        );
        assert!(
            drawing.svg.contains("<g fill=\"#000\">"),
            "{format:?}: dark modules"
        );
        assert!(
            !drawing.svg.contains("href") && !drawing.svg.contains("<image"),
            "{format:?}: no external reference"
        );
        assert_eq!(drawing.quiet_zone, format.quiet_zone());

        // Nothing dark inside the quiet zone: the border strip of the raster is white.
        let scale = 3;
        let (px, w, h) = raster(&drawing, scale);
        let band = drawing.quiet_zone * scale;
        for y in 0..h {
            for x in 0..w {
                if x < band || y < band || x >= w - band || y >= h - band {
                    assert_eq!(px[(y * w + x) as usize], 255, "{format:?} at {x},{y}");
                }
            }
        }
        // The symbol itself is there.
        assert!(px.contains(&0), "{format:?}: dark modules drawn");
    }
}

#[test]
fn the_quiet_zones_match_the_formats() {
    assert_eq!(CodeFormat::Qr.quiet_zone(), 4);
    assert!(CodeFormat::Code128.quiet_zone() >= 10);
    assert!(CodeFormat::Ean13.quiet_zone() >= 9);
}

#[test]
fn a_one_dimensional_code_is_drawn_tall_enough_to_scan() {
    for (format, value) in [
        (CodeFormat::Code128, "ABC-1234567890"),
        (CodeFormat::Ean13, "4006381333931"),
    ] {
        let d = draw(format, value).unwrap();
        let symbol_h = d.height - 2 * d.quiet_zone;
        assert!(symbol_h >= 40, "{format:?}: {symbol_h} modules high");
    }
}

#[test]
fn an_ean13_with_a_wrong_check_digit_is_refused_by_hand() {
    assert_eq!(
        check(CodeFormat::Ean13, "4006381333932"),
        Err(Reason::BadCheckDigit)
    );
    assert_eq!(
        check(CodeFormat::Ean13, "400638133393"),
        Err(Reason::BadLength)
    );
    assert_eq!(
        check(CodeFormat::Ean13, "40063813339X1"),
        Err(Reason::BadCharacters)
    );
    assert!(draw(CodeFormat::Ean13, "4006381333932").is_err());
}

#[test]
fn a_value_that_does_not_fit_its_format_is_refused_with_a_reason() {
    assert_eq!(check(CodeFormat::Qr, ""), Err(Reason::Empty));
    assert_eq!(
        check(CodeFormat::Code128, "Straße"),
        Err(Reason::BadCharacters)
    );
    assert_eq!(
        check(CodeFormat::Qr, &"x".repeat(5000)),
        Err(Reason::TooLong)
    );
    // Text longer than a symbol holds is a reason of its own, not a panic.
    assert_eq!(
        check(CodeFormat::Pdf417, &"ä".repeat(1400)),
        Err(Reason::CannotEncode)
    );
}

#[test]
fn an_unknown_format_text_is_not_a_format() {
    assert_eq!(CodeFormat::parse("qr"), Some(CodeFormat::Qr));
    assert_eq!(CodeFormat::parse("code39"), None);
    for f in CodeFormat::ALL {
        assert_eq!(CodeFormat::parse(f.as_text()), Some(f));
    }
}

#[test]
fn reading_an_image_without_a_code_or_garbage_finds_nothing() {
    assert!(decode(b"not an image").is_empty());
    let white = png(&vec![255u8; 64 * 64], 64, 64);
    assert!(decode(&white).is_empty());
}

#[test]
fn a_found_code_never_prints_its_value() {
    let found = Found {
        format: CodeFormat::Qr,
        value: "SECRET-MARKER-found".to_owned(),
    };
    assert!(!format!("{found:?}").contains("SECRET-MARKER"));
}

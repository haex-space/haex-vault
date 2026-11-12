//! The codes of a pass (spec 092, research R3, FR-003, FR-008, FR-009): one module on `rxing` that
//! checks a code against its format (format rules and check digits), draws it as an SVG (dark on
//! white, with the quiet zone of its format) and reads codes out of an image. It is the one
//! implementation of those rules (a second one in the window would drift). No network, no global
//! state, and no value in an error or a log line: a code is a secret (FR-025).

use std::fmt;

use rxing::{BarcodeFormat, EncodeHints, MultiFormatWriter, Writer};
use serde::Serialize;
use ts_rs::TS;

/// The most pixels of an image `decode` reads: a phone photo is about 12 MP, a page rendered for a
/// scan a few more; a larger image is refused instead of being allocated.
const DECODE_PIXEL_LIMIT: u64 = 64 * 1024 * 1024;
/// The longest value a code of any format may hold (QR: 2953 bytes at the lowest correction).
const VALUE_LIMIT_BYTES: usize = 2900;

/// The formats the system draws and checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeFormat {
    Qr,
    Code128,
    Ean13,
    Pdf417,
    Aztec,
}

impl CodeFormat {
    pub const ALL: [CodeFormat; 5] = [
        CodeFormat::Qr,
        CodeFormat::Code128,
        CodeFormat::Ean13,
        CodeFormat::Pdf417,
        CodeFormat::Aztec,
    ];

    /// The stored text of a format.
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Qr => "qr",
            Self::Code128 => "code128",
            Self::Ean13 => "ean13",
            Self::Pdf417 => "pdf417",
            Self::Aztec => "aztec",
        }
    }

    /// The format of a stored text; `None` for a text of a newer build or an import, which is kept
    /// as it is and flagged `unknown_format`.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_text() == text)
    }

    fn barcode(self) -> BarcodeFormat {
        match self {
            Self::Qr => BarcodeFormat::QR_CODE,
            Self::Code128 => BarcodeFormat::CODE_128,
            Self::Ean13 => BarcodeFormat::EAN_13,
            Self::Pdf417 => BarcodeFormat::PDF_417,
            Self::Aztec => BarcodeFormat::AZTEC,
        }
    }

    fn from_barcode(format: &BarcodeFormat) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.barcode() == *format)
    }

    /// The quiet zone around the symbol, in modules. QR and Code 128 ask for what their
    /// specifications name; EAN-13 for its wider left margin on both sides; PDF417 and Aztec have
    /// a small one in practice.
    pub fn quiet_zone(self) -> u32 {
        match self {
            Self::Qr => 4,
            Self::Code128 => 10,
            Self::Ean13 => 11,
            Self::Pdf417 => 2,
            Self::Aztec => 2,
        }
    }

    fn is_linear(self) -> bool {
        matches!(self, Self::Code128 | Self::Ean13)
    }
}

/// Why a value does not fit its format. The window turns each into a text; none carries the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Empty,
    TooLong,
    /// Code 128 and EAN-13 take only the characters of their alphabet.
    BadCharacters,
    /// EAN-13 has exactly thirteen digits.
    BadLength,
    /// The last digit of an EAN-13 does not match the others.
    BadCheckDigit,
    /// The value is valid text but the format cannot hold it (too much data for the symbol).
    CannotEncode,
    /// The format is not one the system knows.
    UnknownFormat,
}

impl Reason {
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong => "too_long",
            Self::BadCharacters => "bad_characters",
            Self::BadLength => "bad_length",
            Self::BadCheckDigit => "bad_check_digit",
            Self::CannotEncode => "cannot_encode",
            Self::UnknownFormat => "unknown_format",
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_text())
    }
}

/// The drawing of one code: an SVG whose unit is one module, so any size is a scaling of the view
/// box. `width` and `height` are the view box in modules, quiet zone included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct Drawing {
    pub svg: String,
    pub width: u32,
    pub height: u32,
    pub quiet_zone: u32,
}

/// A code found in an image.
#[derive(Clone, PartialEq, Eq)]
pub struct Found {
    pub format: CodeFormat,
    pub value: String,
}

impl fmt::Debug for Found {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Found")
            .field("format", &self.format)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// The EAN-13 check digit of the first twelve digits.
fn ean13_check_digit(first_twelve: &[u8]) -> u8 {
    let sum: u32 = first_twelve
        .iter()
        .enumerate()
        .map(|(i, d)| u32::from(*d) * if i % 2 == 0 { 1 } else { 3 })
        .sum();
    ((10 - sum % 10) % 10) as u8
}

/// Whether a value fits a format: its alphabet and length, the check digit of an EAN-13, and that
/// the symbol can hold it at all.
pub fn check(format: CodeFormat, value: &str) -> Result<(), Reason> {
    if value.is_empty() {
        return Err(Reason::Empty);
    }
    if value.len() > VALUE_LIMIT_BYTES {
        return Err(Reason::TooLong);
    }
    match format {
        CodeFormat::Ean13 => {
            let digits: Vec<u8> = value
                .chars()
                .map(|c| c.to_digit(10).map(|d| d as u8))
                .collect::<Option<_>>()
                .ok_or(Reason::BadCharacters)?;
            if digits.len() != 13 {
                return Err(Reason::BadLength);
            }
            if ean13_check_digit(&digits[..12]) != digits[12] {
                return Err(Reason::BadCheckDigit);
            }
        }
        CodeFormat::Code128 => {
            // Code sets A, B and C together cover ASCII 0 to 127.
            if !value.is_ascii() {
                return Err(Reason::BadCharacters);
            }
        }
        CodeFormat::Qr | CodeFormat::Pdf417 | CodeFormat::Aztec => {}
    }
    matrix(format, value).map(|_| ())
}

/// The symbol at its natural size, one cell per module, with no margin. A 1D symbol is one row
/// high; PDF417 comes with its rows already repeated.
fn matrix(format: CodeFormat, value: &str) -> Result<rxing::common::BitMatrix, Reason> {
    let hints = EncodeHints {
        Margin: Some("0".into()),
        ..Default::default()
    };
    MultiFormatWriter
        .encode_with_hints(value, &format.barcode(), 1, 1, &hints)
        .map_err(|_| Reason::CannotEncode)
}

/// Draws a code: dark modules on a white ground, with the quiet zone of its format around it.
pub fn draw(format: CodeFormat, value: &str) -> Result<Drawing, Reason> {
    check(format, value)?;
    let m = matrix(format, value)?;
    let quiet = format.quiet_zone();
    let (cols, rows) = (m.getWidth(), m.getHeight());
    // A bar of a 1D code is as tall as a scanner needs: a quarter of its width, never below 40.
    let (symbol_h, row_scale) = if format.is_linear() {
        let h = (cols / 4).max(40);
        (h, h)
    } else {
        (rows, 1)
    };
    let (width, height) = (cols + 2 * quiet, symbol_h + 2 * quiet);

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" \
         width=\"{width}\" height=\"{height}\" shape-rendering=\"crispEdges\">\
         <rect width=\"{width}\" height=\"{height}\" fill=\"#fff\"/><g fill=\"#000\">"
    );
    for y in 0..rows {
        let mut x = 0;
        while x < cols {
            if !m.get(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < cols && m.get(x, y) {
                x += 1;
            }
            let (ry, rh) = if format.is_linear() {
                (quiet, row_scale)
            } else {
                (quiet + y, 1)
            };
            svg.push_str(&format!(
                "<rect x=\"{}\" y=\"{ry}\" width=\"{}\" height=\"{rh}\"/>",
                quiet + start,
                x - start
            ));
        }
    }
    svg.push_str("</g></svg>");
    Ok(Drawing {
        svg,
        width,
        height,
        quiet_zone: quiet,
    })
}

/// The codes in an image (a photo or a rendered page), as many as the reader finds. An image that
/// cannot be read, is too large or holds no code gives none. The size is read from the header and
/// checked before the image is decoded, and the decoder gets an allocation limit, so a small file
/// that declares a huge image cannot take the memory of the process.
pub fn decode(image_bytes: &[u8]) -> Vec<Found> {
    let open = || {
        image::ImageReader::new(std::io::Cursor::new(image_bytes))
            .with_guessed_format()
            .ok()
    };
    let Some((w, h)) = open().and_then(|reader| reader.into_dimensions().ok()) else {
        return Vec::new();
    };
    if u64::from(w) * u64::from(h) > DECODE_PIXEL_LIMIT {
        return Vec::new();
    }
    let Some(mut reader) = open() else {
        return Vec::new();
    };
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(DECODE_PIXEL_LIMIT * 4);
    reader.limits(limits);
    let Ok(image) = reader.decode() else {
        return Vec::new();
    };
    let luma = image.to_luma8().into_raw();
    let Ok(results) = rxing::helpers::detect_multiple_in_luma(luma, w, h) else {
        return Vec::new();
    };
    let mut found: Vec<Found> = Vec::new();
    for result in results {
        let Some(format) = CodeFormat::from_barcode(result.getBarcodeFormat()) else {
            continue;
        };
        let code = Found {
            format,
            value: result.getText().to_owned(),
        };
        if !found.contains(&code) {
            found.push(code);
        }
    }
    found
}

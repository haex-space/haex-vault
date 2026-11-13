//! Reading codes from an image for a pass (spec 092, FR-034, research R15): a photo, or one page
//! of a PDF the window rendered to a picture. One decode path in Rust (`codes::decode` on `rxing`)
//! serves both; this module is the thin layer the commands call. It saves nothing: the window shows
//! what was found, the person picks and confirms, and only then a pass is written. A value found
//! is text and is never opened, called or interpreted; no log line or error holds one.

use std::fmt;

use serde::Serialize;
use ts_rs::TS;

use super::codes::{decode, Found};

/// The largest image or PDF read for a scan (the limit of an attachment, 034 FR-020).
pub const MAX_INPUT_BYTES: u64 = 25 * 1024 * 1024;
/// The most codes returned from one image: a sheet holds a few.
const MAX_CODES: usize = 20;

/// One code found in an image, for the person to pick from.
#[derive(Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ScannedCode {
    pub format: String,
    pub value: String,
}

impl fmt::Debug for ScannedCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScannedCode")
            .field("format", &self.format)
            .field("value", &"<redacted>")
            .finish()
    }
}

impl From<Found> for ScannedCode {
    fn from(found: Found) -> Self {
        Self {
            format: found.format.as_text().to_owned(),
            value: found.value,
        }
    }
}

/// The codes an image holds, none for an image that cannot be read or has none.
pub fn scan_image(bytes: &[u8]) -> Vec<ScannedCode> {
    decode(bytes)
        .into_iter()
        .take(MAX_CODES)
        .map(ScannedCode::from)
        .collect()
}

/// Whether the bytes start like a PDF (`%PDF-` within the first kilobyte, as readers accept).
pub fn is_pdf(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    head.windows(5).any(|window| window == b"%PDF-")
}

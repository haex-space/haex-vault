//! Images for models (spec 044 FR-035, research R2): decoded with the limits of the thumbnails,
//! turned upright, the longest edge at most [`IMAGE_EDGE`], encoded as JPEG at the highest quality
//! that stays within [`IMAGE_BYTES`].

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::{ImageReader, RgbImage};

use crate::files::thumbnails::upright_within;
use crate::files::{FilesError, FilesErrorCode};

/// The longest edge of an image a model gets (Anthropic's recommended size).
pub const IMAGE_EDGE: u32 = 1568;

/// The most bytes of an image a model gets (as for attachments, `chat/attachments.rs`).
pub const IMAGE_BYTES: usize = 5 * 1024 * 1024;

/// JPEG qualities tried in turn.
const QUALITIES: [u8; 5] = [85, 75, 65, 50, 35];

/// The image in `bytes` as a JPEG for a model.
pub fn for_model(bytes: &[u8]) -> Result<Vec<u8>, FilesError> {
    let broken = || FilesError::new(FilesErrorCode::Broken, "the image could not be decoded");
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| broken())?;
    let image = upright_within(reader, IMAGE_EDGE).map_err(|_| broken())?;
    encode_within(&image, IMAGE_BYTES).ok_or_else(|| {
        FilesError::new(
            FilesErrorCode::TooLarge,
            "the image stays too large for a model",
        )
    })
}

/// `image` as a JPEG of at most `limit` bytes, at the highest quality that fits.
pub(crate) fn encode_within(image: &RgbImage, limit: usize) -> Option<Vec<u8>> {
    QUALITIES.iter().find_map(|quality| {
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut Cursor::new(&mut bytes), *quality)
            .encode_image(image)
            .ok()?;
        (bytes.len() <= limit).then_some(bytes)
    })
}

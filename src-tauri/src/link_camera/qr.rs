//! Reading the QR code in one JPEG picture of the camera.

use image::ImageFormat;

/// The text of the first QR code in `jpeg`, or `None` when the picture holds none, holds one that
/// does not decode, or is no JPEG (a camera sometimes hands over a torn frame).
pub fn read_qr(jpeg: &[u8]) -> Option<String> {
    let picture = image::load_from_memory_with_format(jpeg, ImageFormat::Jpeg).ok()?;
    let gray = picture.to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(gray);
    prepared
        .detect_grids()
        .into_iter()
        .find_map(|grid| grid.decode().ok().map(|(_, text)| text))
}

#[cfg(test)]
#[path = "qr_tests.rs"]
mod tests;

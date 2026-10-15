//! Checking a PNG or WebP of a set (spec 053 FR-012, research R10). The original bytes are kept; the
//! check only decides whether the file is what its name says and stays within the limits.

use std::io::Cursor;

use image::{ImageFormat, ImageReader, Limits};

use super::ImageMime;

/// The largest side of a raster image of a set.
pub const MAX_SIDE: u32 = 512;
/// Memory a decode may use: a 512 × 512 image in four bytes per pixel is 1 MiB.
const MAX_ALLOC: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterKind {
    Png,
    Webp,
}

impl RasterKind {
    fn from_name(name: &str) -> Option<Self> {
        match crate::files::kind::extension(name).as_str() {
            "png" => Some(Self::Png),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }

    fn format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Webp => ImageFormat::WebP,
        }
    }

    pub fn mime(self) -> ImageMime {
        match self {
            Self::Png => ImageMime::Png,
            Self::Webp => ImageMime::Webp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RasterError {
    #[error("the format is not SVG, PNG or WebP")]
    Format,
    #[error("the file is not the format its name says")]
    Mismatch,
    #[error("the image is larger than 512 x 512 pixels")]
    TooLarge,
    #[error("the image cannot be read")]
    Unreadable,
}

fn reader(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>, RasterError> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| RasterError::Unreadable)
}

/// Whether `bytes` are a readable PNG or WebP that `name` names rightly and that fits the limits.
pub fn check_raster(name: &str, bytes: &[u8]) -> Result<RasterKind, RasterError> {
    let kind = RasterKind::from_name(name).ok_or(RasterError::Format)?;
    let probe = reader(bytes)?;
    if probe.format() != Some(kind.format()) {
        return Err(RasterError::Mismatch);
    }
    let (width, height) = probe
        .into_dimensions()
        .map_err(|_| RasterError::Unreadable)?;
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(RasterError::TooLarge);
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    let mut decode = reader(bytes)?;
    decode.limits(limits);
    decode.decode().map_err(|_| RasterError::Unreadable)?;
    Ok(kind)
}

#[cfg(test)]
#[path = "raster_tests.rs"]
mod tests;

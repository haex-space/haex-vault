//! Reading an icon an extension names (spec 017 notifications, spec 090 header): a `data:` URL of
//! an image or a file of the calling frame's own bundle, checked for its type and size. The one
//! place both callers share, so the rules cannot drift apart; what a caller allows is in its
//! [`IconRules`].

use base64::Engine;

use super::bridge::dispatch::CallContext;
use super::bundle::store::{read_verified_file_within, VerifiedFile};

/// What an icon may be for one caller.
pub struct IconRules {
    /// `(mime type, canonical file extension)` pairs of the accepted pictures.
    pub types: &'static [(&'static str, &'static str)],
    /// Largest picture, decoded.
    pub max_bytes: usize,
    /// Whether a base64 `data:` URL is accepted (a development version has no bundle to read).
    pub inline: bool,
    /// Said when the type is none of `types`.
    pub type_message: &'static str,
}

/// A picture as read.
#[derive(Debug)]
pub struct Icon {
    pub bytes: Vec<u8>,
    /// The mime type of `types` it matched.
    pub mime: &'static str,
    /// The canonical file extension of that type.
    pub extension: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IconError {
    /// Not an icon this caller accepts; the text is for the person who wrote the extension.
    Invalid(&'static str),
    /// A picture over [`IconRules::max_bytes`].
    TooLarge,
}

const SHAPE: &str = "icon must be a base64 data: URL or a bundle file";

/// The type of a file name's extension (`jpeg` counts as `jpg`).
fn type_of_extension(
    path: &str,
    types: &'static [(&'static str, &'static str)],
) -> Option<(&'static str, &'static str)> {
    let (_, extension) = path.rsplit_once('.')?;
    types
        .iter()
        .find(|(_, e)| {
            e.eq_ignore_ascii_case(extension)
                || (*e == "jpg" && extension.eq_ignore_ascii_case("jpeg"))
        })
        .copied()
}

/// Checks and decodes an inline `data:` URL (without reading anything).
fn decode_inline(value: &str, rules: &IconRules) -> Result<Icon, IconError> {
    let data = value
        .strip_prefix("data:")
        .ok_or(IconError::Invalid(SHAPE))?;
    let (mime, encoded) = data
        .split_once(";base64,")
        .ok_or(IconError::Invalid(SHAPE))?;
    let (mime, extension) = rules
        .types
        .iter()
        .find(|(m, _)| m.eq_ignore_ascii_case(mime))
        .copied()
        .ok_or(IconError::Invalid(rules.type_message))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| IconError::Invalid(SHAPE))?;
    if bytes.len() > rules.max_bytes {
        return Err(IconError::TooLarge);
    }
    Ok(Icon {
        bytes,
        mime,
        extension,
    })
}

/// The path of a bundle file as the bundle names it, or why it is none.
fn bundle_path(value: &str) -> Result<String, IconError> {
    if value.contains("://") {
        return Err(IconError::Invalid(SHAPE));
    }
    let path = value.trim_start_matches("./").trim_start_matches('/');
    if path.split('/').any(|part| part == "..") {
        return Err(IconError::Invalid("icon file not in the bundle"));
    }
    Ok(path.to_owned())
}

/// Reads the icon `value` names for the calling frame: a `data:` URL (when `rules.inline`) or a
/// file of the frame's own stored bundle, whose signature is checked on reading. A development
/// version has no stored bundle, so it can only use the inline form.
pub fn read(ctx: &CallContext, value: &str, rules: &IconRules) -> Result<Icon, IconError> {
    if value.starts_with("data:") {
        return if rules.inline {
            decode_inline(value, rules)
        } else {
            Err(IconError::Invalid(SHAPE))
        };
    }
    let path = bundle_path(value)?;
    let (mime, extension) =
        type_of_extension(&path, rules.types).ok_or(IconError::Invalid(rules.type_message))?;
    let bundle_id = ctx
        .session
        .source
        .bundle()
        .ok_or(IconError::Invalid("icon file not in the bundle"))?;
    let max_bytes = rules.max_bytes;
    let bytes = match ctx
        .db
        .read_blocking(move |q| {
            read_verified_file_within(q, bundle_id, &path, max_bytes)
                .map_err(haex_crdt::Error::from)
        })
        .map_err(|_| IconError::Invalid("icon file not readable"))?
    {
        VerifiedFile::Data(bytes) => bytes,
        VerifiedFile::TooLarge => return Err(IconError::TooLarge),
        VerifiedFile::Missing => return Err(IconError::Invalid("icon file not in the bundle")),
    };
    Ok(Icon {
        bytes,
        mime,
        extension,
    })
}

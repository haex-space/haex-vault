//! Sanitizing an SVG (spec 053 FR-010, FR-011, research R9). An SVG is the riskiest image format:
//! it can carry scripts, event handlers, foreign documents and references to the outside. The one
//! rule lives here: only what is on an allow list is written, everything else is dropped with its
//! content. The result is written anew from the parsed events, not patched text. The function is
//! idempotent and runs at the import (only the sanitized form is stored) and again before an image
//! is handed to the window, once per image and set.
//!
//! The window shows the result only as the source of an `<img>`, where no script runs and nothing
//! is fetched; this is a second line, not the only one.

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer, XmlVersion};

/// The largest SVG that is read (spec: 256 KiB per image).
pub const MAX_BYTES: usize = 256 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_ELEMENTS: usize = 20_000;
/// The largest image embedded in an SVG as a `data:` URL.
const MAX_EMBEDDED_BYTES: usize = 128 * 1024;
const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SvgError {
    #[error("the SVG is larger than 256 KiB")]
    TooLarge,
    #[error("the SVG is not UTF-8")]
    NotUtf8,
    #[error("the SVG is not well-formed: {0}")]
    Malformed(String),
    #[error("the SVG has {0}")]
    Forbidden(&'static str),
    #[error("the SVG is nested too deep")]
    TooDeep,
    #[error("the SVG has too many elements")]
    TooManyElements,
    #[error("the SVG has no <svg> root")]
    NoRoot,
}

/// The elements that stay. Everything else (`script`, `foreignObject`, `a`, `animate*`, `set`,
/// `style`, `iframe`, `text` …) goes with its content.
const ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "title",
    "desc",
    "symbol",
    "use",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "linearGradient",
    "radialGradient",
    "stop",
    "clipPath",
    "mask",
    "pattern",
    "image",
    "filter",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
];

/// The attributes that stay: the presentation and geometry attributes of the elements above.
/// `href` and `xlink:href`, `style` and the namespace declarations are handled on their own.
const ATTRIBUTES: &[&str] = &[
    "id",
    "viewBox",
    "width",
    "height",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "fx",
    "fy",
    "fr",
    "d",
    "points",
    "transform",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-opacity",
    "stroke-dasharray",
    "stroke-dashoffset",
    "opacity",
    "clip-path",
    "clip-rule",
    "mask",
    "filter",
    "color",
    "color-interpolation",
    "color-interpolation-filters",
    "display",
    "visibility",
    "overflow",
    "mix-blend-mode",
    "isolation",
    "offset",
    "stop-color",
    "stop-opacity",
    "gradientUnits",
    "gradientTransform",
    "spreadMethod",
    "clipPathUnits",
    "maskUnits",
    "maskContentUnits",
    "patternUnits",
    "patternContentUnits",
    "patternTransform",
    "preserveAspectRatio",
    "version",
    "in",
    "in2",
    "result",
    "stdDeviation",
    "dx",
    "dy",
    "values",
    "type",
    "mode",
    "operator",
    "k1",
    "k2",
    "k3",
    "k4",
    "flood-color",
    "flood-opacity",
    "lighting-color",
    "edgeMode",
    "kernelMatrix",
    "order",
    "scale",
    "xChannelSelector",
    "yChannelSelector",
    "baseFrequency",
    "numOctaves",
    "seed",
    "stitchTiles",
    "surfaceScale",
    "diffuseConstant",
    "specularConstant",
    "specularExponent",
    "tableValues",
    "slope",
    "intercept",
    "amplitude",
    "exponent",
    "radius",
    "filterUnits",
    "primitiveUnits",
    "azimuth",
    "elevation",
    "z",
    "pointsAtX",
    "pointsAtY",
    "pointsAtZ",
    "limitingConeAngle",
];

/// What a `style` attribute may still set (the same presentation properties).
const STYLE_PROPERTIES: &[&str] = &[
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-opacity",
    "stroke-dasharray",
    "stroke-dashoffset",
    "opacity",
    "clip-rule",
    "color",
    "display",
    "visibility",
    "stop-color",
    "stop-opacity",
    "flood-color",
    "flood-opacity",
    "lighting-color",
    "mix-blend-mode",
    "isolation",
];

fn malformed(error: impl std::fmt::Display) -> SvgError {
    SvgError::Malformed(error.to_string())
}

/// Returns the sanitized SVG, or why this file cannot be an icon.
pub fn sanitize_svg(input: &[u8]) -> Result<Vec<u8>, SvgError> {
    if input.len() > MAX_BYTES {
        return Err(SvgError::TooLarge);
    }
    let text = std::str::from_utf8(input).map_err(|_| SvgError::NotUtf8)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = Reader::from_str(text);
    let mut writer = Writer::new(Vec::new());

    // The names of the open elements that were kept, to write their end tags and to know where
    // text may stay.
    let mut open: Vec<String> = Vec::new();
    // Inside a dropped element: how many of its elements are open.
    let mut skipping = 0usize;
    let mut seen_root = false;
    let mut root_closed = false;
    let mut elements = 0usize;

    loop {
        match reader.read_event().map_err(malformed)? {
            Event::Eof => break,
            Event::DocType(_) => return Err(SvgError::Forbidden("a DOCTYPE")),
            Event::PI(_) => return Err(SvgError::Forbidden("a processing instruction")),
            Event::Decl(_) | Event::Comment(_) | Event::CData(_) => {}
            Event::GeneralRef(reference) => {
                let predefined = matches!(&*reference, b"amp" | b"lt" | b"gt" | b"quot" | b"apos");
                if !predefined && !matches!(reference.resolve_char_ref(), Ok(Some(_))) {
                    return Err(SvgError::Forbidden("an entity"));
                }
                if skipping == 0 && keeps_text(&open) {
                    write(&mut writer, Event::GeneralRef(reference))?;
                }
            }
            Event::Text(text) => {
                if skipping == 0 && keeps_text(&open) {
                    write(&mut writer, Event::Text(text))?;
                } else if text.iter().any(|b| !b.is_ascii_whitespace()) && open.is_empty() {
                    return Err(SvgError::Malformed("text outside the root".to_owned()));
                }
            }
            Event::Start(start) => {
                if skipping > 0 {
                    skipping += 1;
                    continue;
                }
                if root_closed {
                    return Err(SvgError::Malformed("content after the root".to_owned()));
                }
                let Some(name) = allowed_name(&start) else {
                    check_root(seen_root, &start)?;
                    skipping = 1;
                    continue;
                };
                check_root_name(seen_root, name)?;
                seen_root = true;
                elements += 1;
                if elements > MAX_ELEMENTS {
                    return Err(SvgError::TooManyElements);
                }
                if open.len() >= MAX_DEPTH {
                    return Err(SvgError::TooDeep);
                }
                let element = rebuild(name, &start, open.is_empty())?;
                write(&mut writer, Event::Start(element))?;
                open.push(name.to_owned());
            }
            Event::Empty(start) => {
                if skipping > 0 {
                    continue;
                }
                if root_closed {
                    return Err(SvgError::Malformed("content after the root".to_owned()));
                }
                let Some(name) = allowed_name(&start) else {
                    check_root(seen_root, &start)?;
                    continue;
                };
                check_root_name(seen_root, name)?;
                elements += 1;
                if elements > MAX_ELEMENTS {
                    return Err(SvgError::TooManyElements);
                }
                let is_root = !seen_root;
                seen_root = true;
                let element = rebuild(name, &start, is_root)?;
                write(&mut writer, Event::Empty(element))?;
                if is_root {
                    root_closed = true;
                }
            }
            Event::End(_) => {
                if skipping > 0 {
                    skipping -= 1;
                    continue;
                }
                let Some(name) = open.pop() else {
                    return Err(SvgError::Malformed("an end tag without a start".to_owned()));
                };
                write(&mut writer, Event::End(BytesEnd::new(name)))?;
                if open.is_empty() {
                    root_closed = true;
                }
            }
        }
    }
    if !seen_root {
        return Err(SvgError::NoRoot);
    }
    if !open.is_empty() {
        return Err(SvgError::Malformed("an element is not closed".to_owned()));
    }
    Ok(writer.into_inner())
}

fn write(writer: &mut Writer<Vec<u8>>, event: Event<'_>) -> Result<(), SvgError> {
    writer.write_event(event).map_err(malformed)
}

/// Text stays only in `title` and `desc`.
fn keeps_text(open: &[String]) -> bool {
    matches!(open.last().map(String::as_str), Some("title" | "desc"))
}

/// The local name of the element when it may stay: no prefix (or `svg:`) and on the list.
fn allowed_name(start: &BytesStart<'_>) -> Option<&'static str> {
    let qname = start.name();
    if let Some(prefix) = qname.prefix() {
        if prefix.as_ref() != b"svg" {
            return None;
        }
    }
    let local_name = qname.local_name();
    let local = std::str::from_utf8(local_name.as_ref()).ok()?;
    ELEMENTS.iter().copied().find(|allowed| *allowed == local)
}

fn check_root(seen_root: bool, _start: &BytesStart<'_>) -> Result<(), SvgError> {
    if seen_root {
        Ok(())
    } else {
        Err(SvgError::NoRoot)
    }
}

fn check_root_name(seen_root: bool, name: &str) -> Result<(), SvgError> {
    if !seen_root && name != "svg" {
        return Err(SvgError::NoRoot);
    }
    Ok(())
}

/// The element with only the attributes that may stay; the root also gets the namespaces, which an
/// image in an `<img>` needs.
fn rebuild(
    name: &'static str,
    start: &BytesStart<'_>,
    is_root: bool,
) -> Result<BytesStart<'static>, SvgError> {
    let mut element = BytesStart::new(name);
    if is_root {
        element.push_attribute(("xmlns", SVG_NS));
        element.push_attribute(("xmlns:xlink", XLINK_NS));
    }
    for attribute in start.attributes() {
        let attribute = attribute.map_err(malformed)?;
        let key = std::str::from_utf8(attribute.key.as_ref()).map_err(|_| SvgError::NotUtf8)?;
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, start.decoder())
            .map_err(malformed)?;
        if let Some(kept) = keep_attribute(name, key, &value) {
            element.push_attribute((key, kept.as_str()));
        }
    }
    Ok(element)
}

/// The value of the attribute when it may stay.
fn keep_attribute(element: &str, key: &str, value: &str) -> Option<String> {
    if key.to_ascii_lowercase().starts_with("on") {
        return None;
    }
    match key {
        "href" | "xlink:href" => keep_reference(element, value),
        "style" => keep_style(value),
        "xml:space" => Some(value.to_owned()),
        _ if ATTRIBUTES.contains(&key) && value_is_safe(value) => Some(value.to_owned()),
        _ => None,
    }
}

/// A reference stays when it points into the image itself (`#id`), or, on an `image`, when it is a
/// small PNG or WebP as a `data:` URL.
fn keep_reference(element: &str, value: &str) -> Option<String> {
    let value = value.trim();
    if let Some(id) = value.strip_prefix('#') {
        let plain = !id.is_empty()
            && id
                .chars()
                .all(|c| !c.is_whitespace() && !"()\"'<>&".contains(c));
        return plain.then(|| value.to_owned());
    }
    if element != "image" {
        return None;
    }
    let data = value
        .strip_prefix("data:image/png;base64,")
        .or_else(|| value.strip_prefix("data:image/webp;base64,"))?;
    let base64 = !data.is_empty()
        && data.len() <= MAX_EMBEDDED_BYTES
        && data
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='));
    base64.then(|| value.to_owned())
}

/// A `style` attribute keeps only the declarations of [`STYLE_PROPERTIES`] with a safe value.
fn keep_style(value: &str) -> Option<String> {
    let kept: Vec<String> = value
        .split(';')
        .filter_map(|declaration| {
            let (property, setting) = declaration.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            let setting = setting.trim();
            (STYLE_PROPERTIES.contains(&property.as_str()) && value_is_safe(setting))
                .then(|| format!("{property}:{setting}"))
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join(";"))
}

/// A value is safe when it neither runs nor fetches anything: no `javascript:`, `@import`,
/// `expression(` or `data:`, and every `url(` points at `#…` inside the image.
fn value_is_safe(value: &str) -> bool {
    let compact: String = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    if [
        "javascript:",
        "vbscript:",
        "@import",
        "expression(",
        "data:",
        "<",
    ]
    .iter()
    .any(|bad| compact.contains(bad))
    {
        return false;
    }
    let mut rest = compact.as_str();
    while let Some(at) = rest.find("url(") {
        let after = rest[at + 4..].trim_start_matches(['"', '\'']);
        if !after.starts_with('#') {
            return false;
        }
        rest = after;
    }
    true
}

#[cfg(test)]
#[path = "svg_tests.rs"]
mod tests;

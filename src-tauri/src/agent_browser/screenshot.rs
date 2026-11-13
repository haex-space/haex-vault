//! `browser.page.screenshot` (spec 065 FR-056 to FR-058, research R21): holzi captures once as PNG,
//! draws the marks itself and encodes one JPEG. The tab remembers the view the image showed, so
//! positions of later tools can be checked against it. A region is a crop at up to twice the
//! density of the full image; positions always stay in the space of the full image.

use serde::Deserialize;
use serde_json::{json, Value};

use super::calls::{browser_error, site_of};
use super::cdp::page::{self, Clip};
use super::cdp::Connection;
use super::error::BrowserError;
use super::frames::{self, FrameAccess};
use super::marks::{self, Mark};
use super::read::{self, Raw};
use super::tab::{PageInfo, Tab};
use super::view::{self, View};
use crate::chat::tools::native_action::{NativeError, NativeReply};
use crate::chat::tools::ToolImage;

/// The most pixels the crop of an element gets as room around it, in CSS pixels.
const PADDING: f64 = 8.0;
/// The smallest region, in image pixels.
const MIN_REGION: f64 = 8.0;
/// How much denser than the full image a region may be.
const MAX_ZOOM: f64 = 2.0;
/// The quality of the JPEG that goes to the model.
const QUALITY: u8 = 70;

/// A part of the full image, in image pixels.
#[derive(Debug, Clone, Copy, Deserialize)]
pub(super) struct Region {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The input of `browser.page.screenshot`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Screenshot {
    pub tab: String,
    pub marks: Option<bool>,
    pub region: Option<Region>,
    pub element: Option<u32>,
}

/// A rectangle in CSS pixels of the view.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn of(value: &Value) -> Option<Self> {
        Some(Self {
            x: value["x"].as_f64()?,
            y: value["y"].as_f64()?,
            w: value["w"].as_f64()?,
            h: value["h"].as_f64()?,
        })
    }

    /// The part of `self` inside `other`, when there is any.
    fn within(self, other: Rect) -> Option<Rect> {
        let (left, top) = (self.x.max(other.x), self.y.max(other.y));
        let right = (self.x + self.w).min(other.x + other.w);
        let bottom = (self.y + self.h).min(other.y + other.h);
        (right > left && bottom > top).then_some(Rect {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        })
    }
}

fn invalid(field: &'static str, expected: &str) -> NativeError {
    NativeError::invalid_input(Some(field), expected)
}

/// The elements of the page with their places, the frames' included, numbered as
/// `browser.page.read` numbers them; the tab learns which numbers belong to which frame.
async fn elements_with_places(
    connection: &Connection,
    tab: &Tab,
    access: &FrameAccess<'_>,
) -> Result<Vec<Value>, NativeError> {
    let reply = read::script(
        connection,
        &tab.session,
        &json!({ "rects": true, "noText": true }),
    )
    .await
    .map_err(browser_error)?;
    let raw: Raw = serde_json::from_value(reply)
        .map_err(|error| browser_error(BrowserError::Protocol(error.to_string())))?;
    let found = frames::read(connection, tab, access, raw.total as u32, true).await;
    tab.set_frames(found.slices);
    let mut elements = raw.elements;
    elements.extend(found.elements);
    Ok(elements)
}

/// The elements to mark, in the order of the page: those with a visible part inside `area` (the
/// whole view, or the region of a crop), at most [`marks::MAX_MARKS`]; and whether there were more.
fn marked(elements: &[Value], area: Rect) -> (Vec<(&Value, Rect)>, bool) {
    let mut seen: Vec<(&Value, Rect)> = elements
        .iter()
        .filter_map(|element| {
            let rect = Rect::of(&element["rect"])?;
            let visible = rect.within(area)?;
            (visible.w * visible.h >= 4.0).then_some((element, visible))
        })
        .collect();
    let more = seen.len() > marks::MAX_MARKS;
    seen.truncate(marks::MAX_MARKS);
    (seen, more)
}

/// How many times a screenshot is taken again when the page changes under it.
const ATTEMPTS: usize = 2;

/// Takes the screenshot `call` asks for; a page that changes while it is taken (another document,
/// another scroll position or size) would put marks of one view on the picture of another, so the
/// shot is taken again, and refused when the page does not hold still.
pub(super) async fn take(
    connection: &Connection,
    tab: &Tab,
    access: &FrameAccess<'_>,
    call: Screenshot,
) -> Result<NativeReply, NativeError> {
    for _ in 0..ATTEMPTS {
        if let Some(reply) = take_once(connection, tab, access, &call).await? {
            return Ok(reply);
        }
    }
    Err(NativeError::new(
        "view_changed",
        "The page changed while the screenshot was taken; take it again.",
    ))
}

/// One try of [`take`]; `None` when the view changed between the start and the capture.
async fn take_once(
    connection: &Connection,
    tab: &Tab,
    access: &FrameAccess<'_>,
    call: &Screenshot,
) -> Result<Option<NativeReply>, NativeError> {
    let shown = view::current(connection, &tab.session)
        .await
        .map_err(browser_error)?;
    let scale = shown.scale();
    let everything = Rect {
        x: 0.0,
        y: 0.0,
        w: shown.width as f64,
        h: shown.height as f64,
    };
    let wants_marks = call.marks.unwrap_or(false);
    let elements = if wants_marks || call.element.is_some() {
        elements_with_places(connection, tab, access).await?
    } else {
        Vec::new()
    };

    // The part of the view to capture, in CSS pixels of the view.
    let area = match (call.region, call.element) {
        (Some(_), Some(_)) => return Err(invalid("region", "either region or element, not both")),
        (Some(region), None) => {
            let wanted = Rect {
                x: region.x / scale,
                y: region.y / scale,
                w: region.width / scale,
                h: region.height / scale,
            };
            if region.width < MIN_REGION || region.height < MIN_REGION {
                return Err(invalid("region", "at least 8 pixels wide and high"));
            }
            wanted
                .within(everything)
                .ok_or_else(|| invalid("region", "a part of the image"))?
        }
        (None, Some(element)) => {
            let rect = elements
                .iter()
                .find(|found| found["n"].as_u64() == Some(u64::from(element)))
                .and_then(|found| Rect::of(&found["rect"]))
                .ok_or_else(|| invalid("element", "an element of the last page result"))?;
            Rect {
                x: rect.x - PADDING,
                y: rect.y - PADDING,
                w: rect.w + 2.0 * PADDING,
                h: rect.h + 2.0 * PADDING,
            }
            .within(everything)
            .ok_or_else(|| invalid("element", "an element in view; scroll it into view first"))?
        }
        (None, None) => everything,
    };
    let cropped = area != everything;
    let capture_scale = if cropped {
        (scale * MAX_ZOOM).min(view::IMAGE_WIDTH / area.w)
    } else {
        scale
    };

    let png = page::capture(
        connection,
        &tab.session,
        Clip {
            x: shown.scroll_x as f64 + area.x,
            y: shown.scroll_y as f64 + area.y,
            width: area.w,
            height: area.h,
            scale: capture_scale,
        },
    )
    .await
    .map_err(browser_error)?;
    // The document, the scroll position and the size must be those of the start, or the picture
    // and the marks (and the view that is stored) would not belong together.
    let after = view::current(connection, &tab.session)
        .await
        .map_err(browser_error)?;
    if after != shown {
        return Ok(None);
    }
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .map_err(|error| browser_error(BrowserError::Protocol(error.to_string())))?
        .to_rgb8();
    // The browser sizes the image by the density of the display as well as by the clip; the marks
    // and the sizes told to the agent are for `capture_scale` image pixels per CSS pixel.
    let mut image = fit(
        image,
        (area.w * capture_scale).round().max(1.0) as u32,
        (area.h * capture_scale).round().max(1.0) as u32,
    );

    let (drawn, more) = if wants_marks {
        marked(&elements, area)
    } else {
        (Vec::new(), false)
    };
    let places: Vec<Mark> = drawn
        .iter()
        .map(|(element, rect)| Mark {
            n: element["n"].as_u64().unwrap_or(0) as u32,
            x: (rect.x - area.x) * capture_scale,
            y: (rect.y - area.y) * capture_scale,
            w: rect.w * capture_scale,
            h: rect.h * capture_scale,
        })
        .collect();
    marks::draw(&mut image, &places);
    let mut jpeg = Vec::new();
    image
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut jpeg, QUALITY,
        ))
        .map_err(|error| browser_error(BrowserError::Protocol(error.to_string())))?;
    let (width, height) = (image.width(), image.height());

    let raw = read::raw(connection, &tab.session, None)
        .await
        .map_err(browser_error)?;
    let site = site_of(&raw.url);
    tab.set_page(PageInfo {
        site: site.clone(),
        title: raw.title.clone(),
    });
    // Positions of later tools are in the space of the full image of this view.
    tab.set_view(Some(shown.clone()));

    let mut content = json!({ "site": site, "url": raw.url, "title": raw.title });
    if wants_marks {
        content["marks"] = drawn
            .iter()
            .map(|(element, _)| {
                json!({
                    "n": element["n"],
                    "kind": element["kind"],
                    "name": element["name"],
                })
            })
            .collect();
        if more {
            content["marks_more"] = Value::Bool(true);
        }
    }
    let mut result = json!({
        "tab": tab.id,
        "untrusted_page_content": content,
        "note": read::NOTE,
        "view": told(&shown),
        "image": { "width": width, "height": height },
    });
    if cropped {
        result["region"] = json!({
            "x": (area.x * scale).round() as i64,
            "y": (area.y * scale).round() as i64,
            "width": (area.w * scale).round() as i64,
            "height": (area.h * scale).round() as i64,
        });
        result["zoom"] = json!((capture_scale / scale * 100.0).round() / 100.0);
    }
    Ok(Some(NativeReply {
        result,
        images: vec![ToolImage {
            name: format!("{}.jpg", tab.id),
            media_type: "image/jpeg".to_owned(),
            bytes: jpeg,
        }],
        target: Some(site),
    }))
}

/// `image` at `width` x `height` pixels: as it is when it already has that size (give or take a
/// pixel of rounding), else scaled to it. A display with a device scale factor above 1 makes the
/// browser return more pixels than the clip and scale ask for.
fn fit(image: image::RgbImage, width: u32, height: u32) -> image::RgbImage {
    if image.width().abs_diff(width) <= 1 && image.height().abs_diff(height) <= 1 {
        return image;
    }
    image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle)
}

fn told(shown: &View) -> Value {
    shown.told()
}

#[cfg(test)]
#[path = "screenshot_tests.rs"]
mod tests;

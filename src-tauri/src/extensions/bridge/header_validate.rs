//! The checks on a header description an extension sends (spec 090-app-header-bar,
//! contracts/header-description.md, research R3), in the contract's order. Pure: nothing is read
//! here; the pictures themselves are checked when they are read (`header_icons.rs`).
//!
//! A description is refused whole with `3001` and a message of the form
//! `header: control "<id>": <what is wrong>`; the message names the control and the field and never
//! quotes what the extension wrote (FR-062 of spec 017). The one exception is a control of a type
//! the catalog does not know: that control alone is left out (FR-021). The result is built from
//! the typed description, so a field the checks do not know never reaches the frontend.

use std::collections::HashSet;
use std::fmt::Display;

use serde_json::{Map, Value};

use super::header::{WmHeaderControl, WmHeaderDescription};
use crate::extensions::error::{BridgeError, ExtensionErrorCode};

pub const MAX_DESCRIPTION_BYTES: usize = 32 * 1024;
pub const MAX_CONTROLS: usize = 32;
pub const MAX_MENU_ENTRIES: usize = 64;
pub const MAX_OPTIONS: usize = 8;
pub const MAX_SEGMENTS: usize = 64;
pub const MAX_TEXT_CHARS: usize = 120;
/// `search.value` and `breadcrumb.editValue`.
pub const MAX_VALUE_CHARS: usize = 1000;
pub const MAX_ID_CHARS: usize = 64;
pub const MAX_ORDER: u64 = 1000;
/// The longest `collapse.group` name, in characters.
pub const MAX_GROUP_CHARS: usize = 64;
/// An inline `data:` icon, as text: twice the decoded limit, as notifications allow.
pub const MAX_INLINE_ICON_CHARS: usize = 2 * 256 * 1024;
/// A path in the bundle.
const MAX_PATH_CHARS: usize = 256;

/// The picture types of an icon, by file extension (SVG is drawn as an image, so it is inert).
pub const ICON_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "svg"];

const CATALOG: &[&str] = &[
    "button",
    "toggle",
    "search",
    "menu",
    "choice",
    "breadcrumb",
    "title",
    "separator",
];
const FORMS: &[&str] = &["overflow", "icon", "shrink", "keep"];
/// Bidirectional formatting characters: they would reorder the text around them.
const BIDI: &[char] = &[
    '\u{061C}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

fn refuse(control: Option<&str>, what: impl Display) -> BridgeError {
    let message = match control {
        Some(id) => format!("header: control \"{id}\": {what}"),
        None => format!("header: {what}"),
    };
    BridgeError::new(ExtensionErrorCode::Validation, message)
}

/// What the checks share: the ids seen so far, and whether an inline picture is allowed.
struct Checker {
    ids: HashSet<String>,
    inline_icons: bool,
}

/// The length of the inline `data:` icons in `value`, which the size limit does not count.
fn inline_icon_len(value: &Value) -> usize {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, v)| match v {
                Value::String(s) if key == "icon" && s.starts_with("data:") => s.len(),
                _ => inline_icon_len(v),
            })
            .sum(),
        Value::Array(items) => items.iter().map(inline_icon_len).sum(),
        _ => 0,
    }
}

/// Checks `params` and returns the description to show. `inline_icons`: the frame is a
/// development version, which has no bundle to read a picture from and may send it inline.
pub fn validate(params: &Value, inline_icons: bool) -> Result<WmHeaderDescription, BridgeError> {
    // 1. size, without the inline pictures.
    let size = params
        .to_string()
        .len()
        .saturating_sub(inline_icon_len(params));
    if size > MAX_DESCRIPTION_BYTES {
        return Err(refuse(None, "the description is larger than 32 KiB"));
    }
    // 2. an object whose areas are absent or lists.
    let object = params
        .as_object()
        .ok_or_else(|| refuse(None, "the description must be an object"))?;
    let mut areas: Vec<Option<&Vec<Value>>> = Vec::new();
    for key in ["start", "center", "end"] {
        areas.push(match object.get(key) {
            None => None,
            Some(Value::Array(items)) => Some(items),
            Some(_) => return Err(refuse(None, format!("{key} must be a list"))),
        });
    }
    // 3. the number of controls, before any is looked at.
    if areas.iter().flatten().map(|a| a.len()).sum::<usize>() > MAX_CONTROLS {
        return Err(refuse(None, "more than 32 controls"));
    }
    let mut checker = Checker {
        ids: HashSet::new(),
        inline_icons,
    };
    let mut built: Vec<Option<Vec<WmHeaderControl>>> = Vec::new();
    for area in areas {
        built.push(match area {
            None => None,
            Some(items) => {
                let mut controls = Vec::new();
                for item in items {
                    if let Some(control) = checker.control(item)? {
                        controls.push(control);
                    }
                }
                Some(controls)
            }
        });
    }
    let mut built = built.into_iter();
    Ok(WmHeaderDescription {
        start: built.next().flatten(),
        center: built.next().flatten(),
        end: built.next().flatten(),
        // 9b: a table the extension sends is ignored; Rust builds it.
        icons: None,
    })
}

impl Checker {
    /// One control; `None` for a type the catalog does not know.
    fn control(&mut self, value: &Value) -> Result<Option<WmHeaderControl>, BridgeError> {
        // 4.
        let object = value
            .as_object()
            .ok_or_else(|| refuse(None, "a control must be an object"))?;
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| refuse(None, "a control needs a type"))?;
        // 5.
        if !CATALOG.contains(&kind) {
            return Ok(None);
        }
        let id = self.identifier(object.get("id"), None)?;
        let own = Some(id.as_str());
        // 6.
        if object.contains_key("testId") {
            return Err(refuse(own, "testId is for holzi's own apps"));
        }
        let named = !matches!(kind, "title" | "separator");
        let label = text(object, "label", own, false)?;
        if named && label.is_none() {
            // 8.
            return Err(refuse(own, "label is needed"));
        }
        text(object, "tooltip", own, true)?;
        let icon = self.icon(object.get("icon"), own)?;
        flag(object, "monochrome", own)?;
        flag(object, "disabled", own)?;
        collapse(object.get("collapse"), own)?;

        if matches!(kind, "button" | "toggle" | "menu") {
            // The text is checked whether or not there is an icon.
            let shown = text(object, "text", own, false)?;
            if icon.is_none() && shown.is_none() {
                return Err(refuse(own, "needs an icon or a text"));
            }
        }
        match kind {
            "toggle" => {
                if !matches!(object.get("pressed"), Some(Value::Bool(_))) {
                    return Err(refuse(own, "pressed must be true or false"));
                }
            }
            "search" => {
                long_text(object, "value", own, true)?
                    .ok_or_else(|| refuse(own, "value must be text"))?;
                text(object, "placeholder", own, true)?;
            }
            "menu" => self.menu_items(object, own)?,
            "choice" => self.options(object, own)?,
            "breadcrumb" => {
                self.segments(object, own)?;
                flag(object, "editable", own)?;
                long_text(object, "editValue", own, true)?;
            }
            "title" => {
                text(object, "text", own, false)?
                    .ok_or_else(|| refuse(own, "text must be 1 to 120 characters"))?;
            }
            _ => {}
        }
        serde_json::from_value::<WmHeaderControl>(value.clone())
            .map(Some)
            .map_err(|_| refuse(own, "has a field of the wrong type"))
    }

    /// 7. `[A-Za-z0-9._:-]{1,64}` and unique in the whole description.
    fn identifier(
        &mut self,
        value: Option<&Value>,
        own: Option<&str>,
    ) -> Result<String, BridgeError> {
        let id = value
            .and_then(Value::as_str)
            .filter(|id| {
                (1..=MAX_ID_CHARS).contains(&id.len())
                    && id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
            })
            .ok_or_else(|| refuse(own, "id must be 1 to 64 letters, digits or . _ : -"))?;
        if !self.ids.insert(id.to_owned()) {
            return Err(refuse(Some(id), "id is used twice"));
        }
        Ok(id.to_owned())
    }

    /// 9. a file of the extension's own bundle, or (development versions only) an inline picture.
    fn icon(
        &self,
        value: Option<&Value>,
        own: Option<&str>,
    ) -> Result<Option<String>, BridgeError> {
        let Some(value) = value.filter(|v| !v.is_null()) else {
            return Ok(None);
        };
        let icon = value
            .as_str()
            .ok_or_else(|| refuse(own, "icon must be text"))?;
        if icon.starts_with("data:") {
            if !self.inline_icons {
                return Err(refuse(
                    own,
                    "an inline icon is only for development versions",
                ));
            }
            if icon.len() > MAX_INLINE_ICON_CHARS {
                return Err(refuse(own, "icon is larger than 256 KiB"));
            }
            return Ok(Some(icon.to_owned()));
        }
        let shaped = !icon.is_empty()
            && icon.len() <= MAX_PATH_CHARS
            && !icon.contains("://")
            && !icon.contains(':')
            && !icon.starts_with("//")
            && !icon.contains('\\')
            && !icon.split('/').any(|part| part == "..");
        if !shaped {
            return Err(refuse(own, "icon must be a picture file of the extension"));
        }
        let extension = icon.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
        if !extension.is_some_and(|e| ICON_EXTENSIONS.contains(&e.as_str())) {
            return Err(refuse(own, "icon must be png, jpeg, webp, gif or svg"));
        }
        Ok(Some(icon.to_owned()))
    }

    fn menu_items(
        &mut self,
        object: &Map<String, Value>,
        own: Option<&str>,
    ) -> Result<(), BridgeError> {
        let items = list(object, "items", own, 0, MAX_MENU_ENTRIES)?;
        for item in items {
            let entry = item
                .as_object()
                .ok_or_else(|| refuse(own, "an entry of items must be an object"))?;
            match entry.get("type").and_then(Value::as_str) {
                Some("separator") => {}
                Some("item") => {
                    self.identifier(entry.get("id"), own)?;
                    text(entry, "label", own, false)?
                        .ok_or_else(|| refuse(own, "an entry needs a label"))?;
                    self.icon(entry.get("icon"), own)?;
                    flag(entry, "disabled", own)?;
                    flag(entry, "checked", own)?;
                }
                _ => return Err(refuse(own, "an entry of items is an item or a separator")),
            }
        }
        Ok(())
    }

    /// 11. also `selected` is one of the options.
    fn options(
        &mut self,
        object: &Map<String, Value>,
        own: Option<&str>,
    ) -> Result<(), BridgeError> {
        let options = list(object, "options", own, 1, MAX_OPTIONS)?;
        let mut ids = Vec::new();
        for option in options {
            let entry = option
                .as_object()
                .ok_or_else(|| refuse(own, "an option must be an object"))?;
            ids.push(self.identifier(entry.get("id"), own)?);
            text(entry, "label", own, false)?
                .ok_or_else(|| refuse(own, "an option needs a label"))?;
            self.icon(entry.get("icon"), own)?;
        }
        let selected = object.get("selected").and_then(Value::as_str);
        if !selected.is_some_and(|s| ids.iter().any(|id| id == s)) {
            return Err(refuse(own, "selected must be the id of an option"));
        }
        Ok(())
    }

    fn segments(
        &mut self,
        object: &Map<String, Value>,
        own: Option<&str>,
    ) -> Result<(), BridgeError> {
        for segment in list(object, "segments", own, 1, MAX_SEGMENTS)? {
            let entry = segment
                .as_object()
                .ok_or_else(|| refuse(own, "a segment must be an object"))?;
            self.identifier(entry.get("id"), own)?;
            text(entry, "label", own, false)?
                .ok_or_else(|| refuse(own, "a segment needs a label"))?;
        }
        Ok(())
    }
}

/// A list field with a count between `min` and `max`.
fn list<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    own: Option<&str>,
    min: usize,
    max: usize,
) -> Result<&'a Vec<Value>, BridgeError> {
    match object.get(key) {
        Some(Value::Array(items)) if (min..=max).contains(&items.len()) => Ok(items),
        _ => Err(refuse(
            own,
            format!("{key} must be a list of {min} to {max}"),
        )),
    }
}

/// No control or bidirectional formatting character (C0, C1, DEL and the BIDI marks).
fn clean(text: &str) -> bool {
    !text.chars().any(|c| c.is_control() || BIDI.contains(&c))
}

/// A text field of up to `max` characters; `None` when absent. Empty only where `empty_ok`.
fn bounded_text(
    object: &Map<String, Value>,
    key: &str,
    own: Option<&str>,
    empty_ok: bool,
    max: usize,
) -> Result<Option<String>, BridgeError> {
    let Some(value) = object.get(key).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let text = value
        .as_str()
        .ok_or_else(|| refuse(own, format!("{key} must be text")))?;
    let length = text.chars().count();
    if length > max || (length == 0 && !empty_ok) {
        let least = if empty_ok { 0 } else { 1 };
        return Err(refuse(
            own,
            format!("{key} must be {least} to {max} characters"),
        ));
    }
    if !clean(text) {
        return Err(refuse(own, format!("{key} holds a control character")));
    }
    Ok(Some(text.to_owned()))
}

fn text(
    object: &Map<String, Value>,
    key: &str,
    own: Option<&str>,
    empty_ok: bool,
) -> Result<Option<String>, BridgeError> {
    bounded_text(object, key, own, empty_ok, MAX_TEXT_CHARS)
}

fn long_text(
    object: &Map<String, Value>,
    key: &str,
    own: Option<&str>,
    empty_ok: bool,
) -> Result<Option<String>, BridgeError> {
    bounded_text(object, key, own, empty_ok, MAX_VALUE_CHARS)
}

fn flag(object: &Map<String, Value>, key: &str, own: Option<&str>) -> Result<(), BridgeError> {
    match object.get(key) {
        None | Some(Value::Null | Value::Bool(_)) => Ok(()),
        Some(_) => Err(refuse(own, format!("{key} must be true or false"))),
    }
}

/// 10. `{order?: integer 0–1000, form?: overflow | icon | shrink | keep, group?: 1–64 characters}`.
fn collapse(value: Option<&Value>, own: Option<&str>) -> Result<(), BridgeError> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(());
    };
    let object = value
        .as_object()
        .ok_or_else(|| refuse(own, "collapse must be an object"))?;
    match object.get("order") {
        None | Some(Value::Null) => {}
        Some(order) if order.as_u64().is_some_and(|n| n <= MAX_ORDER) => {}
        Some(_) => {
            return Err(refuse(
                own,
                "collapse.order must be a whole number 0 to 1000",
            ))
        }
    }
    match object.get("form") {
        None | Some(Value::Null) => {}
        Some(Value::String(form)) if FORMS.contains(&form.as_str()) => {}
        Some(_) => {
            return Err(refuse(
                own,
                "collapse.form must be overflow, icon, shrink or keep",
            ))
        }
    }
    match object.get("group") {
        None | Some(Value::Null) => {}
        Some(Value::String(group)) if (1..=MAX_GROUP_CHARS).contains(&group.chars().count()) => {}
        Some(_) => {
            return Err(refuse(
                own,
                "collapse.group must be a text of 1 to 64 characters",
            ))
        }
    }
    Ok(())
}

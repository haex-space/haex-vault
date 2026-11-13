//! What an app hands holzi for its window's header (spec 090-app-header-bar, data-model.md).
//!
//! The types are the shape of a description. The TypeScript side imports their generated
//! bindings (`WmHeader*`, `src/types/bindings/`); Rust's checks for an extension's description
//! (contracts/header-description.md) are added next to them. A description belongs to one tab and
//! replaces the previous one whole; nothing here is persisted.
//!
//! Fields that name another type are `#[ts(inline)]`: a generated binding that imports another one
//! cannot be resolved by `tsconfig.scripts.json` (research R16), and these types are loaded by the
//! Node check scripts through `lib/wm/types.ts`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use super::dispatch::CallContext;
use super::frames::FrameSource;
use super::header_icons::{icon_names, resolve};
use super::header_validate::validate;
use crate::extensions::error::BridgeError;

/// How a control gives way when the header is too narrow (FR-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum WmHeaderCollapseForm {
    /// Moves into the overflow menu (the default).
    Overflow,
    /// Shrinks to its icon.
    Icon,
    /// Gets narrower, down to a smallest width the type gives.
    Shrink,
    /// Stays in the row as long as anything else can give way.
    Keep,
}

/// The app's say in how one control collapses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct WmHeaderCollapse {
    /// 0 to 1000; the lowest collapses first. Without it the control collapses after every
    /// control that has one, by its place (data-model.md).
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
    #[ts(optional, inline)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<WmHeaderCollapseForm>,
    /// Controls of one area with the same group name move into the overflow menu together: when
    /// one of them gives way, all do (back and forward, say). At most 64 characters.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

/// The fields every control has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct WmHeaderCommon {
    /// Unique in the description; comes back in the events.
    pub id: String,
    /// The accessible name; required for every type except `title` and `separator`.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A system app: an Iconify name (`lucide:…`). An extension: a path in its own bundle.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// System apps only: the test id of the drawn control (default `wm-header-<id>`).
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    /// Draw the picture in the primary colour from its alpha channel (extension pictures).
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monochrome: Option<bool>,
    /// Default is the label.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[ts(optional, inline)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collapse: Option<WmHeaderCollapse>,
}

/// One entry of a `menu` control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum WmHeaderMenuItem {
    Item {
        id: String,
        label: String,
        #[ts(optional)]
        #[serde(skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        #[ts(optional)]
        #[serde(skip_serializing_if = "Option::is_none")]
        disabled: Option<bool>,
        /// A check mark at the end of the row: the entry is a switched-on setting, not an action.
        #[ts(optional)]
        #[serde(skip_serializing_if = "Option::is_none")]
        checked: Option<bool>,
    },
    Separator,
}

/// One option of a `choice` control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct WmHeaderOption {
    pub id: String,
    pub label: String,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// One segment of a `breadcrumb` control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct WmHeaderSegment {
    pub id: String,
    pub label: String,
}

/// One control of the catalog (FR-008). An unknown `type` is left out by the checks (FR-021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum WmHeaderControl {
    /// Events: `click`.
    Button {
        #[serde(flatten)]
        common: WmHeaderCommon,
        #[ts(optional)]
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Events: `toggle` (the new state).
    Toggle {
        #[serde(flatten)]
        common: WmHeaderCommon,
        pressed: bool,
        #[ts(optional)]
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Events: `input` and `submit`.
    Search {
        #[serde(flatten)]
        common: WmHeaderCommon,
        value: String,
        #[ts(optional)]
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    /// Events: `click` (`value` = the entry's id).
    Menu {
        #[serde(flatten)]
        common: WmHeaderCommon,
        #[ts(inline)]
        items: Vec<WmHeaderMenuItem>,
        #[ts(optional)]
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Events: `select` (the option's id).
    Choice {
        #[serde(flatten)]
        common: WmHeaderCommon,
        #[ts(inline)]
        options: Vec<WmHeaderOption>,
        selected: String,
    },
    /// Events: `select` (the segment's id) and, when `editable`, `submit` (the typed path).
    #[serde(rename_all = "camelCase")]
    Breadcrumb {
        #[serde(flatten)]
        common: WmHeaderCommon,
        #[ts(inline)]
        segments: Vec<WmHeaderSegment>,
        #[ts(optional)]
        #[serde(skip_serializing_if = "Option::is_none")]
        editable: Option<bool>,
        #[ts(optional)]
        #[serde(skip_serializing_if = "Option::is_none")]
        edit_value: Option<String>,
    },
    /// Text without an event; replaces the label.
    Title {
        #[serde(flatten)]
        common: WmHeaderCommon,
        text: String,
    },
    Separator {
        #[serde(flatten)]
        common: WmHeaderCommon,
    },
}

/// The description of a window's header: three areas of controls. A missing area is empty.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct WmHeaderDescription {
    #[serde(default)]
    #[ts(optional, inline)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<Vec<WmHeaderControl>>,
    #[serde(default)]
    #[ts(optional, inline)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<Vec<WmHeaderControl>>,
    #[serde(default)]
    #[ts(optional, inline)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<Vec<WmHeaderControl>>,
    /// Added by Rust, never by the app: the pictures not yet sent for this frame, path to
    /// `data:` URL; the frontend merges them into the tab's icon table (research R9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub icons: Option<BTreeMap<String, String>>,
}

/// What the app is told when a control is used (FR-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum WmHeaderEventKind {
    Click,
    Toggle,
    Input,
    Submit,
    Select,
}

/// The value of an event: text for `input`, `submit` and `select`, a flag for `toggle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum WmHeaderValue {
    Text(String),
    Flag(bool),
}

/// The same shape for an extension and a system app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct WmHeaderEvent {
    /// The control's id.
    pub id: String,
    #[ts(inline)]
    pub kind: WmHeaderEventKind,
    /// Absent for the `click` of a button.
    #[ts(optional, inline)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<WmHeaderValue>,
}

/// Where this module lives (contract test, FR-009).
pub const MODULE: &str = module_path!();

/// The method's name on the allowlist; the relay serializes the calls of a frame by it.
pub const METHOD: &str = "extension_header_set";

/// The event to holzi's window: `{frame, seq, header}`.
pub const SET_EVENT: &str = "extension-header-set";

/// `extension_header_set`: checks the description, resolves its pictures from the extension's own
/// bundle and hands the result to the frontend, which shows it in the header of the calling
/// frame's own tab (FR-018, FR-019, FR-023). No permission is needed: it can only change that
/// header. A refused description (`3001`) leaves the previous one in place and emits nothing.
pub fn set(ctx: &CallContext, params: &Value) -> Result<Value, BridgeError> {
    let inline = ctx.session.source == FrameSource::DevServer;
    // One call of a frame at a time (in the order they were received, `HeaderChannel::turn`): a
    // later call always wins, also when an earlier one's picture reads take longer.
    let mut state = ctx.session.header.lock();
    let mut description = validate(params, inline)?;
    let names = icon_names(&description);
    let fresh = resolve(ctx, &names, &state.sent, inline)?;
    state.sent = names.into_iter().collect();
    state.seq += 1;
    description.icons = (!fresh.is_empty()).then_some(fresh);
    ctx.emitter.emit(
        SET_EVENT,
        json!({ "frame": ctx.session.frame, "seq": state.seq, "header": description }),
    );
    Ok(Value::Null)
}

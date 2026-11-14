//! The pinned shortcut of a pass on the home screen of Android (spec 092, FR-038, research R16,
//! `contracts/android-intents.md`). The shortcut opens the app with `haex-vault://pass/<id>`; the
//! id is an opaque UUID and nothing else is in it: no code, no title. The label is what the person
//! typed (the window offers a neutral text, never the title). Where pinning is not supported the
//! window hides the action.

use tauri::AppHandle;
use tauri_plugin_holzi_android::HolziAndroidExt;
use uuid::Uuid;

use crate::error::{HolziError, Result};

/// The longest label; a launcher cuts a long one anyway.
pub const MAX_LABEL_CHARS: usize = 40;

/// What a link to a pass starts with.
pub const PASS_LINK_PREFIX: &str = "haex-vault://pass/";

/// The id of a pass in the form the shortcut carries it: a UUID in its usual hyphenated form,
/// lower case, nothing around it. Anything else is not an id of holzi.
pub fn pass_id(text: &str) -> Option<String> {
    let id = Uuid::parse_str(text).ok()?;
    let canonical = id.hyphenated().to_string();
    text.eq_ignore_ascii_case(&canonical).then_some(canonical)
}

/// The id a `haex-vault://pass/<id>` link names, if it is one and nothing else.
pub fn id_of_link(url: &str) -> Option<String> {
    pass_id(url.strip_prefix(PASS_LINK_PREFIX)?)
}

/// The label for the shortcut: trimmed, without control characters, at most [`MAX_LABEL_CHARS`]
/// characters; `None` if nothing is left.
pub fn clean_label(label: &str) -> Option<String> {
    let clean: String = label
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_LABEL_CHARS)
        .collect();
    let clean = clean.trim();
    (!clean.is_empty()).then(|| clean.to_owned())
}

/// Whether this device can pin a shortcut at all.
#[tauri::command]
pub fn pass_shortcut_supported(app: AppHandle) -> bool {
    app.holzi_android().pin_shortcut_supported()
}

/// Asks the launcher to pin a shortcut that opens the pass `id`. The system asks the person to
/// confirm; nothing is said here when it did.
#[tauri::command]
pub fn pass_shortcut_pin(app: AppHandle, id: String, label: String) -> Result<()> {
    let id = pass_id(&id).ok_or_else(|| HolziError::InvalidInput {
        reason: "not the id of a pass".to_owned(),
    })?;
    let label = clean_label(&label).ok_or_else(|| HolziError::InvalidInput {
        reason: "the shortcut needs a name".to_owned(),
    })?;
    app.holzi_android()
        .pin_pass_shortcut(&id, &label)
        .map_err(|reason| HolziError::Io { reason })
}

#[cfg(test)]
#[path = "pass_shortcut_tests.rs"]
mod tests;

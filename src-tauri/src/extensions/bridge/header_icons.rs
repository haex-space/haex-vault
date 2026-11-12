//! The pictures of an extension's header (spec 090-app-header-bar, research R9): which files a
//! description names, reading them through the shared icon helper, and what holzi remembers per
//! frame so that a file is read and sent once, however often the header changes.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use base64::Engine;

use super::dispatch::CallContext;
use super::header::{WmHeaderControl, WmHeaderDescription, WmHeaderMenuItem};
use super::header_validate::ICON_EXTENSIONS;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::icon_file::{read, IconError, IconRules};
use crate::extensions::notifications::MAX_ICON_BYTES;

/// What the header of a frame remembers; the lock around it is the per-frame order of `set` calls.
#[derive(Debug, Default)]
pub struct HeaderState {
    /// How many descriptions this frame has had emitted; every event carries its number, so a
    /// frontend drops one that is not newer than what it applied.
    pub seq: u64,
    /// The pictures sent for the description now shown (paths, or the inline text); a picture the
    /// next description no longer names is forgotten, so this is bounded by one description.
    pub sent: HashSet<String>,
}

/// The header channel of one frame session (dropped with it).
#[derive(Debug, Default)]
pub struct HeaderChannel(Mutex<HeaderState>);

impl HeaderChannel {
    /// Takes the frame's header lock: calls of one frame are handled one after the other.
    pub fn lock(&self) -> MutexGuard<'_, HeaderState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

const TYPES: &[(&str, &str)] = &[
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/gif", "gif"),
    ("image/svg+xml", "svg"),
];

fn rules(inline: bool) -> IconRules {
    debug_assert!(TYPES.iter().all(|(_, e)| ICON_EXTENSIONS.contains(e)));
    IconRules {
        types: TYPES,
        max_bytes: MAX_ICON_BYTES,
        inline,
        type_message: "icon must be png, jpeg, webp, gif or svg",
    }
}

/// The pictures the description names, each once, in the order they first appear.
pub fn icon_names(description: &WmHeaderDescription) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    let mut note = |icon: &Option<String>| {
        if let Some(icon) = icon {
            if seen.insert(icon.clone()) {
                names.push(icon.clone());
            }
        }
    };
    let areas = [&description.start, &description.center, &description.end];
    for control in areas.into_iter().flatten().flatten() {
        match control {
            WmHeaderControl::Button { common, .. }
            | WmHeaderControl::Toggle { common, .. }
            | WmHeaderControl::Search { common, .. }
            | WmHeaderControl::Breadcrumb { common, .. }
            | WmHeaderControl::Title { common, .. }
            | WmHeaderControl::Separator { common } => note(&common.icon),
            WmHeaderControl::Menu { common, items, .. } => {
                note(&common.icon);
                for item in items {
                    if let WmHeaderMenuItem::Item { icon, .. } = item {
                        note(icon);
                    }
                }
            }
            WmHeaderControl::Choice {
                common, options, ..
            } => {
                note(&common.icon);
                for option in options {
                    note(&option.icon);
                }
            }
        }
    }
    names
}

/// Reads the pictures in `names` that are not in `sent`; `path → data URL`. An inline picture is
/// checked (type, decoded size) and passes as it is. One that cannot be read refuses the whole
/// description.
pub fn resolve(
    ctx: &CallContext,
    names: &[String],
    sent: &HashSet<String>,
    inline: bool,
) -> Result<BTreeMap<String, String>, BridgeError> {
    let rules = rules(inline);
    let mut table = BTreeMap::new();
    for name in names.iter().filter(|n| !sent.contains(*n)) {
        let shown = if name.starts_with("data:") {
            "an inline picture"
        } else {
            name.as_str()
        };
        let icon = read(ctx, name, &rules).map_err(|error| {
            let reason = match error {
                IconError::Invalid(message) => message.to_owned(),
                IconError::TooLarge => "is larger than 256 KiB".to_owned(),
            };
            BridgeError::new(
                ExtensionErrorCode::Validation,
                format!("header: icon {shown}: {reason}"),
            )
        })?;
        let url = if name.starts_with("data:") {
            name.clone()
        } else {
            format!(
                "data:{};base64,{}",
                icon.mime,
                base64::engine::general_purpose::STANDARD.encode(&icon.bytes)
            )
        };
        table.insert(name.clone(), url);
    }
    Ok(table)
}

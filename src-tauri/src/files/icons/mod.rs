//! The icon sets of the file browser (spec 053, research R2 to R9). A set tells which image an
//! entry gets: by its name, its extension or its role (folder, image, a state of encryption).
//! Three sets ship with the app: Lucide (the default and the fallback for every role another set
//! does not cover; it has no images, the window draws its symbols), Phosphor and Material (packs
//! built into the binary). Custom sets are one synced row each ([`store`]).
//!
//! The window asks [`commands::files_icons_resolve`] for the answers of the entries it shows and
//! [`commands::files_icon_image`] for the bytes of an image; the rules of which image an entry
//! gets live here and nowhere else (FR-003). An image is only ever used as the source of an
//! `<img>` (FR-010); an SVG is sanitized before it leaves this module ([`svg`]).

pub mod builtin;
pub mod commands;
pub mod commands_import;
mod import;
mod manifest;
mod pack;
mod raster;
mod resolve;
mod role;
mod sources;
mod store;
mod svg;

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::EncryptedInfo;

pub use pack::{open_pack, pack_dir, Pack, PackError};
pub use resolve::resolve;
pub use role::role_of;

/// The sets that ship with the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BuiltinSet {
    Lucide,
    Phosphor,
    Material,
}

impl BuiltinSet {
    pub const ALL: [Self; 3] = [Self::Lucide, Self::Phosphor, Self::Material];

    pub fn key(self) -> &'static str {
        match self {
            Self::Lucide => "lucide",
            Self::Phosphor => "phosphor",
            Self::Material => "material",
        }
    }
}

/// A set: one of the built-in ones or a custom one by its id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum IconSetRef {
    Builtin { name: BuiltinSet },
    Custom { id: String },
}

/// The set a vault has until it chooses another (operator, 2026-10-15).
pub const DEFAULT_SET: IconSetRef = IconSetRef::Builtin {
    name: BuiltinSet::Material,
};

/// The standard set (Lucide): it fills every role a set does not have and answers for every set
/// that is not available (FR-020).
pub const FALLBACK_SET: IconSetRef = IconSetRef::Builtin {
    name: BuiltinSet::Lucide,
};

/// The key of the vault preference that holds the choice (data-model.md).
pub const SELECTION_KEY: &str = "files.iconSet";

impl IconSetRef {
    /// The value of the preference [`SELECTION_KEY`] for this set.
    pub fn selection_value(&self) -> String {
        match self {
            Self::Builtin { name } => format!("builtin:{}", name.key()),
            Self::Custom { id } => format!("custom:{id}"),
        }
    }

    /// `builtin:<name>` or `custom:<uuid>`; `None` for any other value. A built-in name this app
    /// does not know (a set of a newer version) is also `None`: the caller shows the fallback.
    pub fn parse_selection(value: &str) -> Option<Self> {
        if let Some(name) = value.strip_prefix("builtin:") {
            return BuiltinSet::ALL
                .into_iter()
                .find(|set| set.key() == name)
                .map(|name| Self::Builtin { name });
        }
        let id = value.strip_prefix("custom:")?;
        is_set_id(id).then(|| Self::Custom { id: id.to_owned() })
    }
}

impl FromStr for IconSetRef {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, ()> {
        Self::parse_selection(value).ok_or(())
    }
}

/// Whether `value` has the shape of a set id: a hyphenated UUID.
pub fn is_set_id(value: &str) -> bool {
    value.len() == 36 && uuid::Uuid::parse_str(value).is_ok()
}

/// Whether the preference [`SELECTION_KEY`] may hold `value`: a built-in set this app knows, or a
/// custom set by id. (A value for a built-in set of a newer app is refused here; another device of
/// a newer app writes it and this one shows the fallback, see [`store`].)
pub fn is_valid_selection(value: &str) -> bool {
    IconSetRef::parse_selection(value).is_some()
}

/// The light or dark scheme of the window (FR-006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum Scheme {
    Light,
    Dark,
}

/// What the resolver needs to know of an entry, no more (contracts/tauri-commands.md).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct IconQuery {
    pub name: String,
    pub is_dir: bool,
    pub no_access: bool,
    pub encrypted: Option<EncryptedInfo>,
}

/// The media type of an image of a pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum ImageMime {
    #[serde(rename = "image/svg+xml")]
    #[ts(rename = "image/svg+xml")]
    Svg,
    #[serde(rename = "image/png")]
    #[ts(rename = "image/png")]
    Png,
    #[serde(rename = "image/webp")]
    #[ts(rename = "image/webp")]
    Webp,
}

/// The answer for one entry: an image of the set, or a symbol of the standard set (Lucide, which
/// the window draws in the colour of the text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum IconAnswer {
    /// `id` names the image in the set; [`commands::files_icon_image`] returns its bytes.
    Image {
        id: String,
        mime: ImageMime,
    },
    Builtin {
        name: String,
    },
}

/// Why a set is or is not in use (data-model.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum IconSetState {
    Ready,
    /// A custom set is chosen but its row has not arrived.
    Syncing,
    /// A custom set is chosen and was deleted.
    Missing,
    /// The pack of the row fails the checks (structure, limits, hash).
    Damaged,
    /// A built-in set of a newer app version.
    Unknown,
}

/// One set in the list of the settings (contracts/tauri-commands.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct IconSetInfo {
    #[serde(rename = "ref")]
    #[ts(rename = "ref")]
    pub set_ref: IconSetRef,
    pub name: String,
    pub author: String,
    pub license: String,
    pub source: String,
    pub image_count: u32,
    pub state: IconSetState,
    /// Custom sets can be removed, built-in ones cannot (FR-019).
    pub removable: bool,
}

/// The choice of the vault and what it is in use as: the set and its state (data-model.md). Only
/// `ready` shows the set itself; every other state shows the standard set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct IconSelection {
    pub set: IconSetRef,
    pub state: IconSetState,
}

/// Name, origin and license text of one built-in set, for the page "Lizenzen" (FR-017).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct IconSetLicense {
    pub name: String,
    pub author: String,
    pub license: String,
    pub source: String,
    pub text: String,
}

/// The kind of an entry as far as icons are concerned (research R8). The states come first when an
/// image is chosen: a name or an extension never covers them (FR-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IconRole {
    Folder,
    FolderNoAccess,
    File,
    Image,
    Video,
    Audio,
    Pdf,
    Text,
    EncryptedFolder,
    LockedFolder,
    LockedFile,
    Damaged,
}

impl IconRole {
    /// A state of encryption or of access: it says something about the entry, not what it is.
    pub fn is_state(self) -> bool {
        matches!(
            self,
            Self::FolderNoAccess
                | Self::EncryptedFolder
                | Self::LockedFolder
                | Self::LockedFile
                | Self::Damaged
        )
    }

    /// The key of the role in the block `x-holzi.roles` of a manifest.
    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "folder" => Self::Folder,
            "folderNoAccess" => Self::FolderNoAccess,
            "file" => Self::File,
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "pdf" => Self::Pdf,
            "text" => Self::Text,
            "encryptedFolder" => Self::EncryptedFolder,
            "lockedFolder" => Self::LockedFolder,
            "lockedFile" => Self::LockedFile,
            "damaged" => Self::Damaged,
            _ => return None,
        })
    }

    /// The symbol of the standard set (Lucide) for this role (data-model.md).
    pub fn fallback_symbol(self) -> &'static str {
        match self {
            Self::Folder => "lucide:folder",
            Self::FolderNoAccess | Self::LockedFolder => "lucide:folder-lock",
            Self::File => "lucide:file",
            Self::Image => "lucide:image",
            Self::Video => "lucide:film",
            Self::Audio => "lucide:music",
            Self::Pdf | Self::Text => "lucide:file-text",
            Self::EncryptedFolder => "lucide:folder-key",
            Self::LockedFile => "lucide:file-lock",
            Self::Damaged => "lucide:file-warning",
        }
    }
}

#[cfg(test)]
#[path = "bundled_scheme_tests.rs"]
mod bundled_scheme_tests;
#[cfg(test)]
#[path = "bundled_tests.rs"]
mod bundled_tests;
#[cfg(test)]
mod import_support;
#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

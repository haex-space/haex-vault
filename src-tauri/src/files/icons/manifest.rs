//! The manifest of a set (spec 053 FR-008, research R7): the format of the icon themes of VS Code
//! (`icon-theme.json`) with a block `x-holzi` for what that format lacks (roles, origin). Only what
//! the file browser needs is read; everything else (`folderExpanded`, `languageIds`,
//! `highContrast`, glyphs of an icon font …) is ignored without an error, so a theme of the VS Code
//! world imports as it is.

use std::collections::HashMap;

use serde::Deserialize;

use super::{IconRole, Scheme};

/// The name of the manifest inside a pack.
pub const MANIFEST_NAME: &str = "icon-theme.json";
/// The largest manifest that is read.
pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
/// A role image in the light scheme may have its own definition with this suffix on its id.
const LIGHT_SUFFIX: &str = "-light";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("the manifest is larger than 1 MiB")]
    TooLarge,
    #[error("the manifest is not valid: {0}")]
    Invalid(String),
    #[error("the manifest names an image path that breaks the path rules: {0}")]
    BadPath(String),
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawMaps {
    file: Option<String>,
    folder: Option<String>,
    file_names: HashMap<String, String>,
    file_extensions: HashMap<String, String>,
    folder_names: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawDefinition {
    #[serde(rename = "iconPath")]
    icon_path: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Origin {
    pub author: String,
    pub license: String,
    pub source: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawHolzi {
    roles: HashMap<String, String>,
    origin: Origin,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Raw {
    #[serde(rename = "iconDefinitions")]
    icon_definitions: HashMap<String, RawDefinition>,
    #[serde(flatten)]
    maps: RawMaps,
    light: RawMaps,
    #[serde(rename = "x-holzi")]
    holzi: RawHolzi,
}

/// The lookups of one scheme, resolved to paths inside the pack. Keys are lower case.
#[derive(Debug, Default)]
struct Maps {
    file: Option<String>,
    folder: Option<String>,
    file_names: HashMap<String, String>,
    file_extensions: HashMap<String, String>,
    folder_names: HashMap<String, String>,
}

/// A role image: the one for the dark scheme and, when the set has one, the one for the light.
#[derive(Debug)]
struct RoleImage {
    dark: String,
    light: Option<String>,
}

/// A parsed manifest: what to show for a name, an extension or a role, per scheme.
#[derive(Debug)]
pub struct IconTheme {
    dark: Maps,
    light: Maps,
    roles: HashMap<IconRole, RoleImage>,
    paths: Vec<String>,
    origin: Origin,
}

impl IconTheme {
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(ManifestError::TooLarge);
        }
        let raw: Raw = serde_json::from_slice(bytes)
            .map_err(|error| ManifestError::Invalid(error.to_string()))?;

        // Definitions that name an image; a definition of an icon font glyph has no path and
        // leaves every entry that uses it out.
        let mut images: HashMap<&str, String> = HashMap::new();
        for (id, definition) in &raw.icon_definitions {
            if let Some(path) = &definition.icon_path {
                images.insert(id, normalize(path)?);
            }
        }
        let dark = resolve_maps(&raw.maps, &images);
        let light = resolve_maps(&raw.light, &images);
        let mut roles = HashMap::new();
        for (key, id) in &raw.holzi.roles {
            let (Some(role), Some(path)) = (IconRole::from_key(key), images.get(id.as_str()))
            else {
                continue;
            };
            let light = images.get(format!("{id}{LIGHT_SUFFIX}").as_str()).cloned();
            roles.insert(
                role,
                RoleImage {
                    dark: path.clone(),
                    light,
                },
            );
        }
        let mut paths: Vec<String> = images.into_values().collect();
        paths.sort();
        paths.dedup();
        Ok(Self {
            dark,
            light,
            roles,
            paths,
            origin: raw.holzi.origin,
        })
    }

    /// Every image path the manifest defines; each must exist in the pack.
    pub fn image_paths(&self) -> &[String] {
        &self.paths
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The first hit of `pick` in the maps of the scheme: the light block first, then the main part
    /// (like VS Code, whose block `light` overrides).
    fn first<'a>(
        &'a self,
        scheme: Scheme,
        pick: impl Fn(&'a Maps) -> Option<&'a String>,
    ) -> Option<&'a str> {
        let light = (scheme == Scheme::Light)
            .then(|| pick(&self.light))
            .flatten();
        light.or_else(|| pick(&self.dark)).map(String::as_str)
    }

    /// The image of a file name (given in lower case).
    pub fn file_name(&self, scheme: Scheme, name: &str) -> Option<&str> {
        self.first(scheme, |maps| maps.file_names.get(name))
    }

    /// The image of an extension (lower case, without the dot).
    pub fn extension(&self, scheme: Scheme, extension: &str) -> Option<&str> {
        self.first(scheme, |maps| maps.file_extensions.get(extension))
    }

    /// The image of a folder name (given in lower case).
    pub fn folder_name(&self, scheme: Scheme, name: &str) -> Option<&str> {
        self.first(scheme, |maps| maps.folder_names.get(name))
    }

    /// The default image for files and for folders.
    pub fn default_for(&self, scheme: Scheme, is_dir: bool) -> Option<&str> {
        self.first(scheme, |maps| {
            if is_dir {
                maps.folder.as_ref()
            } else {
                maps.file.as_ref()
            }
        })
    }

    /// The image of a role (`x-holzi.roles`); in the light scheme the one with the suffix `-light`
    /// when the set has it.
    pub fn role(&self, scheme: Scheme, role: IconRole) -> Option<&str> {
        let image = self.roles.get(&role)?;
        match scheme {
            Scheme::Light => image.light.as_ref().or(Some(&image.dark)),
            Scheme::Dark => Some(&image.dark),
        }
        .map(String::as_str)
    }
}

fn resolve_maps(raw: &RawMaps, images: &HashMap<&str, String>) -> Maps {
    let image = |id: &String| images.get(id.as_str()).cloned();
    let table = |map: &HashMap<String, String>| -> HashMap<String, String> {
        map.iter()
            .filter_map(|(key, id)| Some((key.to_lowercase(), image(id)?)))
            .collect()
    };
    Maps {
        file: raw.file.as_ref().and_then(image),
        folder: raw.folder.as_ref().and_then(image),
        file_names: table(&raw.file_names),
        file_extensions: table(&raw.file_extensions),
        folder_names: table(&raw.folder_names),
    }
}

/// The path of an image inside the pack: relative, with `/`, no `.` or `..` segment. A leading
/// `./` is allowed (the format of VS Code writes it); a path that leaves the pack is refused (the
/// import turns `./../icons/x.svg` into `icons/x.svg` before it packs).
fn normalize(path: &str) -> Result<String, ManifestError> {
    let trimmed = path.strip_prefix("./").unwrap_or(path);
    match haex_bundle::format::path_rule_violation(trimmed) {
        None => Ok(trimmed.to_owned()),
        Some(reason) => Err(ManifestError::BadPath(format!("{path:?}: {reason}"))),
    }
}

/// The extensions of a name to try, the longest first: `a.test.ts` gives `test.ts`, then `ts`. A
/// leading dot is part of the name (`.gitignore` has no extension).
pub fn extension_candidates(name: &str) -> impl Iterator<Item = &str> {
    name.char_indices()
        .filter(|(at, c)| *c == '.' && *at > 0)
        .map(move |(at, _)| &name[at + 1..])
        .filter(|extension| !extension.is_empty())
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;

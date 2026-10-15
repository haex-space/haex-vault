//! The sets that ship with the app (spec 053 FR-007, research R6): Lucide (the standard set, no
//! images) and the packs of Phosphor and Material, built into the binary. The packs are made from
//! the source directories under `src-tauri/icon-sets/` by [`super::pack_dir`] (see the test
//! `build_bundled_packs` in `builtin_tests.rs`), so the same code reads them as reads a custom set.

use std::sync::{Arc, LazyLock};

use super::pack::open_pack;
use super::{BuiltinSet, IconSetInfo, IconSetLicense, IconSetRef, IconSetState, Pack};

static PHOSPHOR_ZIP: &[u8] = include_bytes!("../../../icon-sets/phosphor.zip");
static MATERIAL_ZIP: &[u8] = include_bytes!("../../../icon-sets/material.zip");
const LUCIDE_LICENSE: &str = include_str!("../../../icon-sets/lucide.LICENSE");

static PHOSPHOR: LazyLock<Option<Arc<Pack>>> = LazyLock::new(|| open(PHOSPHOR_ZIP, "Phosphor"));
static MATERIAL: LazyLock<Option<Arc<Pack>>> = LazyLock::new(|| open(MATERIAL_ZIP, "Material"));

fn open(bytes: &[u8], name: &str) -> Option<Arc<Pack>> {
    match open_pack(bytes, None) {
        Ok(pack) => Some(Arc::new(pack)),
        Err(error) => {
            // A bundled pack that does not open is a bug of the build; `every_bundled_pack_passes_the_checks`
            // keeps it from shipping. The window shows the standard set meanwhile.
            log::error!("icon sets: the bundled pack {name} does not open: {error}");
            None
        }
    }
}

/// The opened pack of a built-in set; `None` for Lucide, which has none.
pub fn pack(name: BuiltinSet) -> Option<Arc<Pack>> {
    match name {
        BuiltinSet::Lucide => None,
        BuiltinSet::Phosphor => PHOSPHOR.clone(),
        BuiltinSet::Material => MATERIAL.clone(),
    }
}

/// The raw bytes of a bundled pack, for the tests.
#[cfg(test)]
pub fn zip_bytes(name: BuiltinSet) -> &'static [u8] {
    match name {
        BuiltinSet::Lucide => &[],
        BuiltinSet::Phosphor => PHOSPHOR_ZIP,
        BuiltinSet::Material => MATERIAL_ZIP,
    }
}

fn display_name(name: BuiltinSet) -> &'static str {
    match name {
        BuiltinSet::Lucide => "Lucide",
        BuiltinSet::Phosphor => "Phosphor",
        BuiltinSet::Material => "Material",
    }
}

/// The built-in sets for the list of the settings, in the order of [`BuiltinSet::ALL`].
pub fn infos() -> Vec<IconSetInfo> {
    BuiltinSet::ALL
        .into_iter()
        .map(|name| {
            let (author, license, source, image_count, state) = match name {
                BuiltinSet::Lucide => (
                    "Lucide Contributors".to_owned(),
                    "ISC AND MIT".to_owned(),
                    "https://github.com/lucide-icons/lucide".to_owned(),
                    0,
                    IconSetState::Ready,
                ),
                _ => match pack(name) {
                    Some(pack) => {
                        let origin = pack.theme().origin();
                        (
                            origin.author.clone(),
                            origin.license.clone(),
                            origin.source.clone(),
                            pack.image_count(),
                            IconSetState::Ready,
                        )
                    }
                    None => (
                        String::new(),
                        String::new(),
                        String::new(),
                        0,
                        IconSetState::Damaged,
                    ),
                },
            };
            IconSetInfo {
                set_ref: IconSetRef::Builtin { name },
                name: display_name(name).to_owned(),
                author,
                license,
                source,
                image_count,
                state,
                removable: false,
            }
        })
        .collect()
}

/// Author, license and the full text of each built-in set (FR-017).
pub fn licenses() -> Vec<IconSetLicense> {
    infos()
        .into_iter()
        .map(|info| {
            let text = match &info.set_ref {
                IconSetRef::Builtin {
                    name: BuiltinSet::Lucide,
                } => LUCIDE_LICENSE.trim().to_owned(),
                IconSetRef::Builtin { name } => pack(*name)
                    .map(|pack| pack.license_text())
                    .unwrap_or_default(),
                IconSetRef::Custom { .. } => String::new(),
            };
            IconSetLicense {
                name: info.name,
                author: info.author,
                license: info.license,
                source: info.source,
                text,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "builtin_tests.rs"]
mod tests;

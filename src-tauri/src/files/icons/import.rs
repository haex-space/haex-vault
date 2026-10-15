//! The import of a custom set (spec 053 US3, research R6): a folder, an archive or a VSIX becomes
//! one pack. The manifest is found (`icon-theme.json`, or the path `contributes.iconThemes[0]` of a
//! `package.json` names), every image it names is read, checked and, for an SVG, sanitized, and
//! what cannot be taken is left out with its reason instead of failing the whole set. The result
//! is a new manifest that names only the images that are in the pack, so the pack opens with the
//! same loader as every other (`pack::open_pack`).

use std::collections::{BTreeMap, BTreeSet};

use haex_bundle::format::PathChecker;
use serde::Serialize;
use serde_json::{json, Map, Value};
use ts_rs::TS;

use super::manifest::{MANIFEST_NAME, MAX_MANIFEST_BYTES};
use super::pack::{limits, open_pack, pack_files, pack_hash, PackError};
use super::raster::check_raster;
use super::sources::Files;
use super::svg::sanitize_svg;
use super::IconRole;

/// How many custom sets a vault holds.
pub const MAX_SETS: usize = 10;
/// The largest license text taken into the pack.
const LICENSE_BYTES: u64 = 128 * 1024;
const LICENSE_FILES: usize = 6;

/// One sentence per reason; the window shows it as it is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("The source cannot be read: {0}")]
    Unreadable(String),
    #[error("The archive is larger than 64 MiB.")]
    ArchiveTooLarge,
    #[error("The source has more than 20000 files.")]
    TooManyEntries,
    #[error("The archive holds a path that leaves the set: {0:?}.")]
    UnsafePath(String),
    #[error("The archive unpacks to more than 200 times its size.")]
    Ratio,
    #[error("The set has no icon-theme.json and no package.json that names one.")]
    NoManifest,
    #[error("The manifest cannot be used: {0}")]
    BadManifest(String),
    #[error("The set has more than 2000 images.")]
    TooManyImages,
    #[error("The images of the set hold more than 5 MiB.")]
    TooMuchData,
    #[error("The set has no image that can be used.")]
    NoUsableImage,
    #[error("The set cannot be packed: {0}")]
    Pack(String),
    #[error("A vault holds at most 10 own icon sets; remove one first.")]
    TooManySets,
    #[error("This device cannot read a folder; choose an archive.")]
    FolderUnavailable,
    #[error("The set to replace does not exist.")]
    NoSuchSet,
}

impl From<PackError> for ImportError {
    fn from(error: PackError) -> Self {
        Self::Pack(error.to_string())
    }
}

/// A file the import left out and why (`IconImportReport.skipped`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct IconImportSkipped {
    pub path: String,
    pub reason: String,
}

/// What an import would write, for the dialog before it does (contracts/tauri-commands.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct IconImportReport {
    pub name: String,
    pub author: String,
    pub license: String,
    pub image_count: u32,
    pub bytes: u64,
    pub skipped: Vec<IconImportSkipped>,
    /// The id of a custom set of the vault with the same name, if there is one.
    pub same_name: Option<String>,
}

/// A checked set, ready to be written as one row.
#[derive(Debug)]
pub struct Prepared {
    pub report: IconImportReport,
    pub source: String,
    pub pack: Vec<u8>,
    pub sha256: String,
}

fn skip(skipped: &mut Vec<IconImportSkipped>, path: &str, reason: &str) {
    skipped.push(IconImportSkipped {
        path: path.to_owned(),
        reason: reason.to_owned(),
    });
}

/// `rel` as a path inside the set when read from the folder `dir` (both with `/`); `None` when it
/// climbs out of the set or is absolute. A leading `./` and inner `..` that stay inside are folded.
fn resolve_path(dir: &str, rel: &str) -> Option<String> {
    if rel.starts_with('/') || rel.contains('\\') {
        return None;
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The base of the set in the source (`""`, `"extension/"` of a VSIX, or the one folder an archive
/// of a download holds), the path of the manifest and the `package.json` beside it, if any.
fn locate(files: &dyn Files) -> Result<(String, String, Option<Value>), ImportError> {
    let names = files.files();
    let mut bases = vec![String::new(), "extension/".to_owned()];
    let tops: BTreeSet<&str> = names
        .keys()
        .filter_map(|name| name.split_once('/').map(|(top, _)| top))
        .collect();
    if tops.len() == 1 {
        bases.extend(tops.into_iter().map(|top| format!("{top}/")));
    }
    for base in bases {
        let package = names
            .contains_key(&format!("{base}package.json"))
            .then(|| files.read(&format!("{base}package.json"), 1024 * 1024))
            .transpose()?
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        let named = package
            .as_ref()
            .and_then(|package| package["contributes"]["iconThemes"][0]["path"].as_str())
            .and_then(|path| resolve_path(&base, path))
            .filter(|path| names.contains_key(path));
        let manifest = named.or_else(|| {
            let root = format!("{base}{MANIFEST_NAME}");
            names.contains_key(&root).then_some(root)
        });
        if let Some(manifest) = manifest {
            return Ok((base, manifest, package));
        }
    }
    Err(ImportError::NoManifest)
}

fn text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value["name"].as_str())
        .or_else(|| value["url"].as_str())
        .or_else(|| value["type"].as_str())
}

/// A line of text for the report: no control characters, at most `max` characters.
fn line(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// The ids a manifest maps, from a map of `id` or of a name to `id`, kept when `keep` holds.
fn kept_map(value: &Value, keep: &BTreeSet<&str>) -> Value {
    let mut out = Map::new();
    if let Some(map) = value.as_object() {
        for (key, id) in map {
            if id.as_str().is_some_and(|id| keep.contains(id)) {
                out.insert(key.clone(), id.clone());
            }
        }
    }
    Value::Object(out)
}

/// Whether the lookups of a scheme map nothing.
fn is_empty(maps: &Value) -> bool {
    maps.as_object().is_none_or(|maps| {
        maps.values().all(|value| match value {
            Value::Object(map) => map.is_empty(),
            _ => false,
        })
    })
}

/// One scheme's lookups of the manifest reduced to the images that are in the pack.
fn kept_maps(source: &Value, keep: &BTreeSet<&str>) -> Value {
    let mut out = Map::new();
    for key in ["file", "folder"] {
        if let Some(id) = source[key].as_str().filter(|id| keep.contains(id)) {
            out.insert(key.to_owned(), json!(id));
        }
    }
    for key in ["fileNames", "fileExtensions", "folderNames"] {
        out.insert(key.to_owned(), kept_map(&source[key], keep));
    }
    Value::Object(out)
}

/// The images that go into the pack by their path, and the path of the image of each definition.
type TakenImages = (BTreeMap<String, Vec<u8>>, BTreeMap<String, String>);

/// Reads and checks every image the manifest names; returns the pack paths by their bytes and the
/// id of each definition that has an image in the pack.
fn take_images(
    files: &dyn Files,
    base: &str,
    manifest_dir: &str,
    definitions: &Map<String, Value>,
    skipped: &mut Vec<IconImportSkipped>,
) -> Result<TakenImages, ImportError> {
    let mut paths: BTreeMap<String, Option<String>> = BTreeMap::new();
    for definition in definitions.values() {
        if let Some(written) = definition["iconPath"].as_str() {
            let inside = resolve_path(manifest_dir, written)
                .and_then(|path| path.strip_prefix(base).map(str::to_owned));
            paths.entry(written.to_owned()).or_insert(inside);
        }
    }
    let distinct: BTreeSet<&String> = paths.values().flatten().collect();
    if distinct.len() > limits::IMAGES {
        return Err(ImportError::TooManyImages);
    }
    let mut checker = PathChecker::default();
    let mut taken: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut decided: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut total = 0u64;
    for (written, inside) in &paths {
        let Some(path) = inside else {
            skip(skipped, written, "the path leaves the set");
            continue;
        };
        if let Some(known) = decided.get(path) {
            decided.insert(written.clone(), known.clone());
            continue;
        }
        let full = format!("{base}{path}");
        let outcome = (|| -> Result<Vec<u8>, String> {
            if files.is_link(&full) {
                return Err("a symbolic link is not followed".to_owned());
            }
            let Some(size) = files.files().get(&full) else {
                return Err("not found in the set".to_owned());
            };
            let extension = crate::files::kind::extension(path);
            if !matches!(extension.as_str(), "svg" | "png" | "webp") {
                return Err("only SVG, PNG and WebP images are taken".to_owned());
            }
            if *size > limits::IMAGE_BYTES {
                return Err("larger than 256 KiB".to_owned());
            }
            let bytes = files
                .read(&full, limits::IMAGE_BYTES)
                .map_err(|error| error.to_string())?;
            let bytes = if extension == "svg" {
                sanitize_svg(&bytes).map_err(|error| error.to_string())?
            } else {
                check_raster(path, &bytes).map_err(|error| error.to_string())?;
                bytes
            };
            checker.check(path).map_err(|error| error.to_string())?;
            Ok(bytes)
        })();
        match outcome {
            Ok(bytes) => {
                total += bytes.len() as u64;
                if total > limits::UNPACKED_BYTES {
                    return Err(ImportError::TooMuchData);
                }
                taken.insert(path.clone(), bytes);
                decided.insert(written.clone(), Some(path.clone()));
                decided.insert(path.clone(), Some(path.clone()));
            }
            Err(reason) => {
                skip(skipped, written, &reason);
                decided.insert(written.clone(), None);
                decided.insert(path.clone(), None);
            }
        }
    }
    let ids = definitions
        .iter()
        .filter_map(|(id, definition)| {
            let written = definition["iconPath"].as_str()?;
            let path = decided.get(written)?.as_ref()?;
            Some((id.clone(), path.clone()))
        })
        .collect();
    Ok((taken, ids))
}

/// The license and notice files at the root of the set, named `LICENSE`, `LICENSE-2` … and
/// `NOTICE` in the pack.
fn license_files(files: &dyn Files, base: &str) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let (mut licenses, mut notices) = (0, 0);
    for (name, size) in files.files() {
        let Some(own) = name.strip_prefix(base).filter(|own| !own.contains('/')) else {
            continue;
        };
        let lower = own.to_lowercase();
        let target = if lower.starts_with("license")
            || lower.starts_with("licence")
            || lower.starts_with("copying")
        {
            licenses += 1;
            if licenses == 1 {
                "LICENSE".to_owned()
            } else {
                format!("LICENSE-{licenses}")
            }
        } else if lower.starts_with("notice") {
            notices += 1;
            if notices == 1 {
                "NOTICE".to_owned()
            } else {
                continue;
            }
        } else {
            continue;
        };
        if *size > LICENSE_BYTES || out.len() >= LICENSE_FILES {
            continue;
        }
        if let Ok(bytes) = files.read(name, LICENSE_BYTES) {
            if std::str::from_utf8(&bytes).is_ok() {
                out.insert(target, bytes);
            }
        }
    }
    out
}

/// Reads `files` and builds the pack of a custom set; writes nothing. `fallback_name` is the name
/// of the file or folder, used when the set names itself nowhere.
pub fn prepare(files: &dyn Files, fallback_name: &str) -> Result<Prepared, ImportError> {
    let (base, manifest_path, package) = locate(files)?;
    let manifest_bytes = files.read(&manifest_path, MAX_MANIFEST_BYTES as u64)?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| ImportError::BadManifest(error.to_string()))?;
    let empty = Map::new();
    let definitions = manifest["iconDefinitions"].as_object().unwrap_or(&empty);
    let manifest_dir = parent(&manifest_path);

    let mut skipped = Vec::new();
    let (images, ids) = take_images(files, &base, manifest_dir, definitions, &mut skipped)?;
    if images.is_empty() {
        return Err(ImportError::NoUsableImage);
    }
    let keep: BTreeSet<&str> = ids.keys().map(String::as_str).collect();

    let mut theme = Map::new();
    let defined: Map<String, Value> = ids
        .iter()
        .map(|(id, path)| (id.clone(), json!({ "iconPath": path })))
        .collect();
    theme.insert("iconDefinitions".to_owned(), Value::Object(defined));
    if let Value::Object(maps) = kept_maps(&manifest, &keep) {
        theme.extend(maps);
    }
    let light = kept_maps(&manifest["light"], &keep);
    if !is_empty(&light) {
        theme.insert("light".to_owned(), light);
    }
    let mut roles = Map::new();
    if let Some(given) = manifest["x-holzi"]["roles"].as_object() {
        for (key, id) in given {
            if IconRole::from_key(key).is_some() && id.as_str().is_some_and(|id| keep.contains(id))
            {
                roles.insert(key.clone(), id.clone());
            }
        }
    }

    let origin = &manifest["x-holzi"]["origin"];
    let pick = |own: Option<&str>, fallback: &Value, max: usize| {
        line(
            own.filter(|v| !v.trim().is_empty())
                .or_else(|| text(fallback))
                .unwrap_or(""),
            max,
        )
    };
    let package = package.unwrap_or(Value::Null);
    let author = pick(
        text(&package["publisher"]).or_else(|| text(&package["author"])),
        &origin["author"],
        120,
    );
    let license = pick(text(&package["license"]), &origin["license"], 200);
    let source = pick(
        text(&package["repository"]).or_else(|| text(&package["homepage"])),
        &origin["source"],
        500,
    );
    theme.insert(
        "x-holzi".to_owned(),
        json!({ "roles": roles, "origin": { "author": author, "license": license, "source": source } }),
    );
    let name = [
        text(&package["displayName"]),
        text(&package["name"]),
        Some(fallback_name),
    ]
    .into_iter()
    .flatten()
    .map(|candidate| line(candidate, 80))
    .find(|candidate| !candidate.is_empty())
    .unwrap_or_else(|| "Icon-Set".to_owned());

    let mut pack_files_map: BTreeMap<String, Vec<u8>> = images;
    pack_files_map.extend(license_files(files, &base));
    pack_files_map.insert(
        MANIFEST_NAME.to_owned(),
        serde_json::to_vec(&Value::Object(theme))
            .map_err(|error| ImportError::BadManifest(error.to_string()))?,
    );
    let pack = pack_files(&pack_files_map)?;
    let opened = open_pack(&pack, None)?;
    skipped.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Prepared {
        report: IconImportReport {
            name,
            author,
            license,
            image_count: opened.image_count(),
            bytes: pack.len() as u64,
            skipped,
            same_name: None,
        },
        source,
        sha256: pack_hash(&pack),
        pack,
    })
}

/// The time `secs` after the epoch as `YYYY-MM-DDTHH:MM:SSZ` (the column `created_at`).
pub fn iso8601(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    // Civil date from a day count (H. Hinnant's algorithm).
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;

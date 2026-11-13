//! Reading a `.pkpass` file (spec 092, research R6, FR-028 to FR-033). The file is a ZIP from a
//! stranger, so it is read as untrusted input: a hard limit on the number of entries and on the
//! size of each entry and of all of them, a limit on how far an entry may be inflated, names with
//! `..`, an absolute path or a drive refused, nothing extracted to disk, nothing executed, and no
//! address in the file called (FR-029). Only the few entries holzi uses are read, each through a
//! bounded reader, so a size the central directory lies about cannot fill the memory.
//!
//! [`read`] returns the pass in the shape the rest of the system knows ([`ParsedPass`]); the mapping
//! of `pass.json` lives in `pkpass_map.rs`. The parts the optional signature check needs (the
//! manifest and the signature) travel with the result.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use serde::Serialize;
use ts_rs::TS;

use super::model::{CodeInput, PassKind, Start};
use crate::error::HolziError;
use crate::passwords::model::KeyValueInput;

/// The most entries a pass file may hold: a real one has a dozen (images, strings, a manifest).
const MAX_ENTRIES: usize = 256;
/// The most one entry may inflate to.
const MAX_ENTRY_BYTES: u64 = 4 * 1024 * 1024;
/// The most all entries together may inflate to.
const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
/// How many times an entry may be larger than what it is stored as, once it is large enough for
/// this to matter. A zero-filled image shrinks a thousandfold, so the check starts above a small
/// size.
const MAX_RATIO: u64 = 200;
const RATIO_FLOOR_BYTES: u64 = 256 * 1024;
/// The longest `pass.json` read.
const MAX_JSON_BYTES: u64 = 1024 * 1024;
/// The longest picture kept as the icon.
const MAX_ICON_BYTES: u64 = 512 * 1024;

/// Why a file is not read as a pass: the reason a refusal carries (FR-033).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    NotAZip,
    UnsafeName,
    TooManyEntries,
    TooLarge,
    Inflated,
    NoPassJson,
    BadPassJson,
    NoStyle,
    UnsupportedVersion,
}

impl Refusal {
    pub fn as_text(self) -> &'static str {
        match self {
            Self::NotAZip => "not_a_zip",
            Self::UnsafeName => "unsafe_name",
            Self::TooManyEntries => "too_many_entries",
            Self::TooLarge => "too_large",
            Self::Inflated => "inflated",
            Self::NoPassJson => "no_pass_json",
            Self::BadPassJson => "bad_pass_json",
            Self::NoStyle => "no_style",
            Self::UnsupportedVersion => "unsupported_version",
        }
    }
}

impl From<Refusal> for HolziError {
    fn from(refusal: Refusal) -> Self {
        HolziError::PassRefused {
            code: "not_a_pass_file".to_owned(),
            position: None,
            reason: Some(refusal.as_text().to_owned()),
        }
    }
}

/// One thing in the file that holzi does not keep, named in the report (FR-028, FR-032): `what` is
/// the name in the file, `why` a short code the window turns into a sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct NotKept {
    pub what: String,
    pub why: String,
}

/// Why something is not kept: the update service of the pass is never called.
pub const WHY_UPDATE_SERVICE: &str = "update_service";
/// Why something is not kept: holzi does not use it (a place, a colour, a picture of the layout).
pub const WHY_NOT_SUPPORTED: &str = "not_supported";

/// A pass as read from a file, before the user has seen it.
#[derive(Debug, Clone)]
pub struct ParsedPass {
    pub pass_kind: PassKind,
    /// Whether the pass is a ticket or a boarding pass (the place rule of FR-019).
    pub ticket_like: bool,
    pub title: String,
    pub provider: Option<String>,
    pub start: Option<Start>,
    /// `YYYY-MM-DD`.
    pub expires_at: Option<String>,
    pub codes: Vec<CodeInput>,
    pub key_values: Vec<KeyValueInput>,
    /// The bytes of the logo, a PNG within the limit.
    pub icon: Option<Vec<u8>>,
    /// `passTypeIdentifier/serialNumber`: the identity of the file for the duplicate check (R7).
    pub source_id: Option<String>,
    pub not_kept: Vec<NotKept>,
    /// The manifest and the signature, for the optional check (FR-030).
    pub signed: Option<SignedParts>,
}

/// What the optional signature check needs, read from the same file.
#[derive(Debug, Clone)]
pub struct SignedParts {
    pub manifest: Vec<u8>,
    pub signature: Vec<u8>,
    /// Every entry of the file that is not the manifest or the signature, by name, for the hashes
    /// the manifest names. Entries over the limit are not here and make the check not checkable.
    pub files: HashMap<String, Vec<u8>>,
}

/// The entries of the file that `pkpass_map` reads, by name.
pub(super) struct Entries {
    pub pass_json: Vec<u8>,
    /// `<language>.lproj/pass.strings`, by the part before `.lproj`.
    pub strings: HashMap<String, Vec<u8>>,
    pub logo: Option<Vec<u8>>,
    pub signed: Option<SignedParts>,
    /// Names of pictures that are in the file and not kept (for the report).
    pub other_images: Vec<String>,
}

/// Reads a pass file. `language` is the language of the window (`de`, `en`): the strings of that
/// language replace the base text, and the base text is the fallback.
pub fn read(bytes: &[u8], language: &str) -> Result<ParsedPass, Refusal> {
    let entries = open(bytes)?;
    super::pkpass_map::map(entries, language)
}

/// Opens the ZIP, applies every limit and reads the entries holzi uses.
pub(super) fn open(bytes: &[u8]) -> Result<Entries, Refusal> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| Refusal::NotAZip)?;
    if archive.len() > MAX_ENTRIES {
        return Err(Refusal::TooManyEntries);
    }
    let mut declared_total = 0u64;
    let mut names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|_| Refusal::NotAZip)?;
        let name = file.name().to_owned();
        if !safe_name(&name) {
            return Err(Refusal::UnsafeName);
        }
        if file.is_dir() {
            continue;
        }
        let size = file.size();
        if size > MAX_ENTRY_BYTES {
            return Err(Refusal::TooLarge);
        }
        declared_total = declared_total.saturating_add(size);
        if declared_total > MAX_TOTAL_BYTES {
            return Err(Refusal::TooLarge);
        }
        let stored = file.compressed_size();
        if size > RATIO_FLOOR_BYTES && (stored == 0 || size / stored > MAX_RATIO) {
            return Err(Refusal::Inflated);
        }
        names.push((index, name));
    }

    let mut pass_json = None;
    let mut manifest = None;
    let mut signature = None;
    let mut strings = HashMap::new();
    let mut files = HashMap::new();
    let mut logo: Option<(u8, Vec<u8>)> = None;
    let mut other_images = Vec::new();
    // What has really been inflated so far: the sizes in the file are only what it says.
    let mut read_total = 0u64;
    for (index, name) in names {
        let lower = name.to_ascii_lowercase();
        let wanted_json = lower == "pass.json";
        let size_limit = if wanted_json {
            MAX_JSON_BYTES
        } else {
            MAX_ENTRY_BYTES
        }
        .min(MAX_TOTAL_BYTES - read_total);
        let mut file = archive.by_index(index).map_err(|_| Refusal::NotAZip)?;
        let mut data = Vec::new();
        // One byte more than the limit shows that the entry is longer than it said.
        file.by_ref()
            .take(size_limit + 1)
            .read_to_end(&mut data)
            .map_err(|_| Refusal::NotAZip)?;
        if data.len() as u64 > size_limit {
            return Err(Refusal::TooLarge);
        }
        read_total += data.len() as u64;
        if lower == "manifest.json" {
            manifest = Some(data);
            continue;
        }
        if lower == "signature" {
            signature = Some(data);
            continue;
        }
        if wanted_json {
            pass_json = Some(data.clone());
        } else if let Some(language) = lower
            .strip_suffix(".lproj/pass.strings")
            .filter(|l| !l.contains('/'))
        {
            strings.insert(language.to_owned(), data.clone());
        } else if let Some(rank) = logo_rank(&lower) {
            if data.len() as u64 <= MAX_ICON_BYTES && is_small_png(&data) {
                if logo.as_ref().is_none_or(|(best, _)| rank < *best) {
                    logo = Some((rank, data.clone()));
                }
            } else {
                other_images.push(name.clone());
            }
        } else if is_image(&lower) {
            other_images.push(name.clone());
        }
        // Every entry but the manifest and the signature is covered by the manifest's hashes.
        files.insert(name, data);
    }
    let pass_json = pass_json.ok_or(Refusal::NoPassJson)?;
    let signed = match (manifest, signature) {
        (Some(manifest), Some(signature)) => Some(SignedParts {
            manifest,
            signature,
            files,
        }),
        _ => None,
    };
    other_images.sort();
    Ok(Entries {
        pass_json,
        strings,
        logo: logo.map(|(_, data)| data),
        signed,
        other_images,
    })
}

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
/// The longest side of a picture kept as the icon, in pixels. The size in bytes alone says little
/// (a PNG of a few kilobytes can name a picture of gigabytes), so the size the header names is
/// read and limited too; a real logo is a few hundred pixels.
const MAX_ICON_SIDE: u32 = 2048;

/// Whether the bytes start as a PNG whose header names a picture within the limit. The picture is
/// not decoded here; its header is the part that decides how much memory showing it takes.
fn is_small_png(data: &[u8]) -> bool {
    let (Some(width), Some(height)) = (data.get(16..20), data.get(20..24)) else {
        return false;
    };
    let side = |bytes: &[u8]| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    data.starts_with(PNG_MAGIC)
        && data.get(12..16) == Some(b"IHDR")
        && (1..=MAX_ICON_SIDE).contains(&side(width))
        && (1..=MAX_ICON_SIDE).contains(&side(height))
}

/// How good an entry is as the icon: the smaller the better; `None` for any other file. The logo
/// of the pass first, the icon of the notification as the fallback.
fn logo_rank(lower: &str) -> Option<u8> {
    match lower {
        "logo@2x.png" => Some(0),
        "logo.png" => Some(1),
        "icon@2x.png" => Some(2),
        "icon.png" => Some(3),
        _ => None,
    }
}

fn is_image(lower: &str) -> bool {
    [".png", ".jpg", ".jpeg", ".gif"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// A name that stays inside the archive: no `..`, no absolute path, no drive, no backslash trick,
/// no control character.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.contains('\\')
        && !name.chars().any(char::is_control)
        && !name
            .split('/')
            .any(|part| part == ".." || part.ends_with(':'))
}

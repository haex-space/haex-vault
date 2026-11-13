//! Step one of the pkpass signature check (spec 092, T067): the files of the pass against the hashes
//! in `manifest.json` (a JSON object, file name to the SHA-1 of the file in hex).

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use super::signature::Fail;
use super::signature_algs::Hash;

/// Every file the manifest names is in the pass with the same hash, and every file in the pass is
/// named. A manifest that cannot be read is `not_checkable`; the rest is `invalid`.
pub(super) fn check_files(manifest: &[u8], files: &HashMap<String, Vec<u8>>) -> Result<(), Fail> {
    let Ok(Value::Object(listed)) = serde_json::from_slice::<Value>(manifest) else {
        return Err(Fail::unreadable());
    };
    // Sorted, so the same file reports the same reason on every run.
    let mut hashes = BTreeMap::new();
    for (name, value) in &listed {
        let Value::String(hex) = value else {
            return Err(Fail::unreadable());
        };
        hashes.insert(name.as_str(), hex.to_ascii_lowercase());
    }
    for (name, wanted) in &hashes {
        let Some(data) = files.get(*name) else {
            return Err(Fail::invalid("file_missing"));
        };
        // Apple names SHA-1; a SHA-256 hash is told by its length and read the same way.
        let hash = match wanted.len() {
            40 => Hash::Sha1,
            64 => Hash::Sha256,
            _ => return Err(Fail::invalid("file_changed")),
        };
        if hex_of(&hash.digest(data)) != *wanted {
            return Err(Fail::invalid("file_changed"));
        }
    }
    if files.keys().any(|name| !hashes.contains_key(name.as_str())) {
        return Err(Fail::invalid("file_not_in_manifest"));
    }
    Ok(())
}

fn hex_of(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

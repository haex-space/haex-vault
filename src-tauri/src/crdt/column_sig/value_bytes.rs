use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

const CANONICAL_QUIET_NAN: u64 = 0x7FF8_0000_0000_0000;

/// Storage-class tag prefixed to every canonical value encoding.
///
/// Values mirror SQLite's own `SQLITE_*` type codes so the tag is
/// recognisable against the C API: INTEGER=1, FLOAT=2, TEXT=3, BLOB=4,
/// NULL=5.
pub mod tag {
    pub const INTEGER: u8 = 1;
    pub const REAL: u8 = 2;
    pub const TEXT: u8 = 3;
    pub const BLOB: u8 = 4;
    pub const NULL: u8 = 5;
}

/// SQLite storage class carried alongside a column signature.
///
/// Tauri IPC and JSON erase distinctions that are part of the signature
/// preimage: an integer-valued REAL becomes an ordinary JS number and a BLOB
/// becomes a base64 string. Keeping the original class in the signed record
/// lets every receiver reconstruct the exact canonical bytes. Tampering with
/// this field only makes verification fail because the class tag is itself the
/// first signed byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageClass {
    Integer,
    Real,
    Text,
    Blob,
    Null,
}

impl StorageClass {
    pub fn of(value: &Value) -> Self {
        match value {
            Value::Integer(_) => Self::Integer,
            Value::Real(_) => Self::Real,
            Value::Text(_) => Self::Text,
            Value::Blob(_) => Self::Blob,
            Value::Null => Self::Null,
        }
    }

    /// Reconstruct the original SQLite value from its JSON/IPC projection.
    pub fn restore(self, value: &JsonValue) -> Result<Value, String> {
        match self {
            Self::Null if value.is_null() => Ok(Value::Null),
            Self::Null => Err("NULL storage class carried a non-null value".to_string()),
            Self::Integer => value
                .as_i64()
                .map(Value::Integer)
                .or_else(|| value.as_bool().map(|v| Value::Integer(i64::from(v))))
                .ok_or_else(|| "INTEGER storage class carried a non-integer value".to_string()),
            Self::Real => value
                .as_f64()
                .map(Value::Real)
                .ok_or_else(|| "REAL storage class carried a non-number value".to_string()),
            Self::Text => value
                .as_str()
                .map(|v| Value::Text(v.to_string()))
                .ok_or_else(|| "TEXT storage class carried a non-string value".to_string()),
            Self::Blob => {
                if let Some(encoded) = value.as_str() {
                    return BASE64
                        .decode(encoded)
                        .map(Value::Blob)
                        .map_err(|e| format!("BLOB value is not base64: {e}"));
                }
                let bytes = value
                    .as_array()
                    .ok_or_else(|| {
                        "BLOB storage class carried neither base64 nor bytes".to_string()
                    })?
                    .iter()
                    .map(|entry| {
                        entry
                            .as_u64()
                            .filter(|byte| *byte <= u8::MAX as u64)
                            .map(|byte| byte as u8)
                            .ok_or_else(|| "BLOB array contains a non-byte value".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Blob(bytes))
            }
        }
    }
}

/// Canonical byte encoding of a SQLite value for use as a signature preimage
/// component.
///
/// Layout: `storage_class_tag (1 byte) || native byte form`.
///
/// The tag is load-bearing, not decoration. Length-prefix framing in
/// [`super::preimage::build_preimage`] separates *fields*; it does not
/// separate *types* within one field. Untagged, `NULL`, `TEXT("")` and
/// `BLOB([])` all encode to the same empty byte string, and `Integer(1)`
/// collides with `Blob([0,0,0,0,0,0,0,1])` and with any `Text` carrying those
/// same 8 bytes. Because the signature covers only these bytes, an attacker
/// could replay a signature captured over one value while substituting a
/// same-byte value of a different storage class — the `NULL` → `""` swap
/// being the easy one. Tagging gives each storage class a distinct preimage.
///
/// - `Null` → `[NULL]`
/// - `Integer(i64)` → `[INTEGER]` + big-endian 8 bytes
/// - `Real(f64)` → `[REAL]` + big-endian IEEE-754 bits, with NaN normalised
///   to a canonical quiet-NaN pattern and -0.0 normalised to +0.0
/// - `Text(String)` → `[TEXT]` + UTF-8 bytes verbatim (no Unicode
///   normalisation)
/// - `Blob(Vec<u8>)` → `[BLOB]` + bytes verbatim
///
/// Mirrored byte-for-byte by
/// `src/utils/columnSigCanonical.ts::toCanonicalBytes` and by the fixture
/// generator `scripts/gen-column-sig-vectors.ts`. Any drift breaks
/// cross-language verification.
pub fn to_canonical_bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::Null => vec![tag::NULL],
        Value::Integer(i) => tagged(tag::INTEGER, &i.to_be_bytes()),
        Value::Real(f) => {
            let bits = if f.is_nan() {
                CANONICAL_QUIET_NAN
            } else if *f == 0.0 {
                // Kills -0.0 as well since -0.0 == 0.0 under PartialEq.
                0u64
            } else {
                f.to_bits()
            };
            tagged(tag::REAL, &bits.to_be_bytes())
        }
        Value::Text(s) => tagged(tag::TEXT, s.as_bytes()),
        Value::Blob(b) => tagged(tag::BLOB, b),
    }
}

fn tagged(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + body.len());
    out.push(tag);
    out.extend_from_slice(body);
    out
}

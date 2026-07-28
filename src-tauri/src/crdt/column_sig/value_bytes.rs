use rusqlite::types::Value;

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

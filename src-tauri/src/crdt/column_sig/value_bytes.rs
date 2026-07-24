use rusqlite::types::Value;

const CANONICAL_QUIET_NAN: u64 = 0x7FF8_0000_0000_0000;

/// Canonical byte encoding of a SQLite value for use as a signature preimage
/// component.
///
/// Design principle: typeless native byte form per SQLite storage class, no
/// type-tag prefix. Cross-type collision protection is provided by the
/// length-prefixed framing of the full column-sig preimage (Task A2), not by
/// this function.
///
/// - `Null` → empty byte string
/// - `Integer(i64)` → big-endian 8 bytes
/// - `Real(f64)` → big-endian IEEE-754 bits, with NaN normalised to a canonical
///   quiet-NaN pattern and -0.0 normalised to +0.0
/// - `Text(String)` → UTF-8 bytes verbatim (no Unicode normalisation)
/// - `Blob(Vec<u8>)` → bytes verbatim
pub fn to_canonical_bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::Null => Vec::new(),
        Value::Integer(i) => i.to_be_bytes().to_vec(),
        Value::Real(f) => {
            let bits = if f.is_nan() {
                CANONICAL_QUIET_NAN
            } else if *f == 0.0 {
                // Kills -0.0 as well since -0.0 == 0.0 under PartialEq.
                0u64
            } else {
                f.to_bits()
            };
            bits.to_be_bytes().to_vec()
        }
        Value::Text(s) => s.as_bytes().to_vec(),
        Value::Blob(b) => b.clone(),
    }
}

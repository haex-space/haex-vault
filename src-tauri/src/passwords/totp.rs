//! TOTP for the entries (spec 034, FR-003, research R8): RFC 6238 over `hmac` with `sha1` and
//! `sha2`, a hand-written Base32 (RFC 4648) and the parsing of a bare secret or an `otpauth://`
//! address. The code is computed in Rust; the secret never goes to the webview.
//!
//! A value that arrives through sync or an import can be invalid, so [`otp_state`] and the other
//! readers never panic on a stored value (`NULL`, zero, negative numbers, unknown algorithm).

use std::fmt;

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use zeroize::Zeroizing;

use super::model::OtpState;
use crate::error::{HaexError, Result};

pub const DEFAULT_DIGITS: u32 = 6;
pub const DEFAULT_PERIOD: u32 = 30;
const MIN_DIGITS: i64 = 6;
const MAX_DIGITS: i64 = 10;
const MIN_PERIOD: i64 = 1;
const MAX_PERIOD: i64 = 300;
const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

impl OtpAlgorithm {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OtpAlgorithm::Sha1 => "SHA1",
            OtpAlgorithm::Sha256 => "SHA256",
            OtpAlgorithm::Sha512 => "SHA512",
        }
    }

    /// `SHA1`, `SHA256` or `SHA512`, in any case; `None` for anything else.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_uppercase().as_str() {
            "SHA1" => Some(OtpAlgorithm::Sha1),
            "SHA256" => Some(OtpAlgorithm::Sha256),
            "SHA512" => Some(OtpAlgorithm::Sha512),
            _ => None,
        }
    }
}

/// What a code needs. The secret is zeroed when dropped and `Debug` prints no value.
pub struct OtpParams {
    pub secret: Zeroizing<Vec<u8>>,
    pub digits: u32,
    pub period: u32,
    pub algorithm: OtpAlgorithm,
}

impl fmt::Debug for OtpParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OtpParams")
            .field("secret", &"<redacted>")
            .field("digits", &self.digits)
            .field("period", &self.period)
            .field("algorithm", &self.algorithm)
            .finish()
    }
}

impl OtpParams {
    /// The secret in its stored form: upper-case Base32 without spaces or padding.
    pub fn normalised_secret(&self) -> Zeroizing<String> {
        Zeroizing::new(encode_base32(&self.secret))
    }
}

fn invalid(field: &str) -> HaexError {
    HaexError::InvalidInput {
        reason: field.to_string(),
    }
}

/// Decodes RFC 4648 Base32: case-insensitive, spaces and `=` padding are ignored. `None` for a
/// character outside the alphabet or an input that decodes to nothing.
pub fn decode_base32(input: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(input.len() * 5 / 8);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for c in input.chars() {
        if c == ' ' || c == '=' || c == '-' {
            continue;
        }
        let value = BASE32
            .iter()
            .position(|&b| char::from(b) == c.to_ascii_uppercase())? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    if bytes.is_empty() {
        None
    } else {
        Some(bytes)
    }
}

/// Encodes to upper-case Base32 without padding.
pub fn encode_base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 8 / 5 + 1);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(BASE32[((buffer >> bits) & 31) as usize]));
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(char::from(BASE32[((buffer << (5 - bits)) & 31) as usize]));
    }
    out
}

/// `%XX` escapes of an `otpauth://` query value; anything that is not a valid escape stays as it
/// is.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The pieces of an `otpauth://totp/...?secret=...` address.
struct OtpAuth {
    secret: Zeroizing<String>,
    digits: Option<i64>,
    period: Option<i64>,
    algorithm: Option<String>,
}

fn parse_otpauth(address: &str) -> Result<OtpAuth> {
    let rest = address
        .trim()
        .strip_prefix("otpauth://")
        .ok_or_else(|| invalid("otpSecret"))?;
    let (kind, rest) = rest.split_once('/').ok_or_else(|| invalid("otpSecret"))?;
    if !kind.eq_ignore_ascii_case("totp") {
        return Err(invalid("otpSecret"));
    }
    let query = rest.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut auth = OtpAuth {
        secret: Zeroizing::new(String::new()),
        digits: None,
        period: None,
        algorithm: None,
    };
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = percent_decode(value);
        match key.to_ascii_lowercase().as_str() {
            "secret" => auth.secret = Zeroizing::new(value),
            "digits" => auth.digits = Some(value.trim().parse().map_err(|_| invalid("otpDigits"))?),
            "period" => auth.period = Some(value.trim().parse().map_err(|_| invalid("otpPeriod"))?),
            "algorithm" => auth.algorithm = Some(value),
            _ => {}
        }
    }
    if auth.secret.trim().is_empty() {
        return Err(invalid("otpSecret"));
    }
    Ok(auth)
}

/// Reads a secret input and the optional explicit parts into the parameters of a code. The input
/// is a bare Base32 secret or an `otpauth://` address; an explicit part wins over the address,
/// missing parts get 6 digits, a period of 30 seconds and SHA-1. Errors name the field:
/// `otpSecret`, `otpDigits` (6–10), `otpPeriod` (1–300) or `otpAlgorithm`.
pub fn resolve(
    input: &str,
    digits: Option<i64>,
    period: Option<i64>,
    algorithm: Option<&str>,
) -> Result<OtpParams> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(invalid("otpSecret"));
    }
    let (secret_text, address) = if trimmed.starts_with("otpauth://") {
        let auth = parse_otpauth(trimmed)?;
        (auth.secret.clone(), Some(auth))
    } else {
        (Zeroizing::new(trimmed.to_string()), None)
    };
    let secret = decode_base32(&secret_text).ok_or_else(|| invalid("otpSecret"))?;
    let (digits, period, algorithm) = validate_parts(
        digits.or(address.as_ref().and_then(|a| a.digits)),
        period.or(address.as_ref().and_then(|a| a.period)),
        algorithm.or(address.as_ref().and_then(|a| a.algorithm.as_deref())),
    )?;
    Ok(OtpParams {
        secret: Zeroizing::new(secret),
        digits,
        period,
        algorithm,
    })
}

/// Checks digits (6–10), period (1–300) and algorithm and fills in the defaults 6, 30 and SHA-1
/// for what is missing. Errors name the field.
pub fn validate_parts(
    digits: Option<i64>,
    period: Option<i64>,
    algorithm: Option<&str>,
) -> Result<(u32, u32, OtpAlgorithm)> {
    let digits = digits.unwrap_or(i64::from(DEFAULT_DIGITS));
    let period = period.unwrap_or(i64::from(DEFAULT_PERIOD));
    if !(MIN_DIGITS..=MAX_DIGITS).contains(&digits) {
        return Err(invalid("otpDigits"));
    }
    if !(MIN_PERIOD..=MAX_PERIOD).contains(&period) {
        return Err(invalid("otpPeriod"));
    }
    let algorithm = match algorithm {
        Some(name) => OtpAlgorithm::parse(name).ok_or_else(|| invalid("otpAlgorithm"))?,
        None => OtpAlgorithm::Sha1,
    };
    Ok((digits as u32, period as u32, algorithm))
}

fn hmac_of<M: Mac + KeyInit>(key: &[u8], message: &[u8]) -> Vec<u8> {
    // `new_from_slice` accepts a key of any length for HMAC.
    match <M as KeyInit>::new_from_slice(key) {
        Ok(mut mac) => {
            mac.update(message);
            mac.finalize().into_bytes().to_vec()
        }
        Err(_) => Vec::new(),
    }
}

/// The code for a Unix time (RFC 6238 over RFC 4226 dynamic truncation), zero-padded to `digits`.
pub fn code_at(params: &OtpParams, unix_seconds: u64) -> String {
    let counter = (unix_seconds / u64::from(params.period.max(1))).to_be_bytes();
    let digest = match params.algorithm {
        OtpAlgorithm::Sha1 => hmac_of::<Hmac<Sha1>>(&params.secret, &counter),
        OtpAlgorithm::Sha256 => hmac_of::<Hmac<Sha256>>(&params.secret, &counter),
        OtpAlgorithm::Sha512 => hmac_of::<Hmac<Sha512>>(&params.secret, &counter),
    };
    let Some(&last) = digest.last() else {
        return String::new();
    };
    let offset = usize::from(last & 0x0f);
    let truncated = digest
        .get(offset..offset + 4)
        .map(|b| u32::from_be_bytes([b[0] & 0x7f, b[1], b[2], b[3]]))
        .unwrap_or(0);
    let modulus = 10u64.pow(params.digits.min(10));
    format!(
        "{:0width$}",
        u64::from(truncated) % modulus,
        width = params.digits as usize
    )
}

/// Seconds until the code changes, `1..=period`.
pub fn remaining_seconds(period: u32, now: u64) -> u32 {
    let period = u64::from(period.max(1));
    (period - now % period) as u32
}

/// The parameters of a stored entry: the secret as stored plus the columns, `NULL` meaning the
/// defaults. Errors are those of [`resolve`].
pub fn stored_params(
    secret: &str,
    digits: Option<i64>,
    period: Option<i64>,
    algorithm: Option<&str>,
) -> Result<OtpParams> {
    resolve(secret, digits, period, algorithm)
}

/// Whether the stored values can produce a code. Never fails: anything unusable is `Invalid`.
pub fn otp_state(
    secret: Option<&str>,
    digits: Option<i64>,
    period: Option<i64>,
    algorithm: Option<&str>,
) -> OtpState {
    match secret {
        Some(text) if !text.trim().is_empty() => {
            match stored_params(text, digits, period, algorithm) {
                Ok(_) => OtpState::Valid,
                Err(_) => OtpState::Invalid,
            }
        }
        _ => OtpState::None,
    }
}

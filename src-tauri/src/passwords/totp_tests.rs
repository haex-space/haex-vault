//! Tests for TOTP (spec 034, FR-003, SC-003, research R8): the RFC 6238 vectors, the Base32 and
//! `otpauth://` input, the limits, and the state of a stored value that may be invalid.

use super::model::OtpState;
use super::totp::{
    code_at, decode_base32, encode_base32, otp_state, remaining_seconds, resolve, OtpAlgorithm,
    OtpParams,
};
use zeroize::Zeroizing;

const SHA1_KEY: &[u8] = b"12345678901234567890";
const SHA256_KEY: &[u8] = b"12345678901234567890123456789012";
const SHA512_KEY: &[u8] = b"1234567890123456789012345678901234567890123456789012345678901234";

fn params(key: &[u8], algorithm: OtpAlgorithm, digits: u32) -> OtpParams {
    OtpParams {
        secret: Zeroizing::new(key.to_vec()),
        digits,
        period: 30,
        algorithm,
    }
}

/// RFC 6238 appendix B: (time, SHA-1, SHA-256, SHA-512), 8 digits, period 30.
const VECTORS: [(u64, &str, &str, &str); 6] = [
    (59, "94287082", "46119246", "90693936"),
    (1111111109, "07081804", "68084774", "25091201"),
    (1111111111, "14050471", "67062674", "99943326"),
    (1234567890, "89005924", "91819424", "93441116"),
    (2000000000, "69279037", "90698825", "38618901"),
    (20000000000, "65353130", "77737706", "47863826"),
];

#[test]
fn the_rfc_6238_vectors_match_for_all_three_algorithms() {
    for (time, sha1, sha256, sha512) in VECTORS {
        assert_eq!(
            code_at(&params(SHA1_KEY, OtpAlgorithm::Sha1, 8), time),
            sha1,
            "SHA-1 at {time}"
        );
        assert_eq!(
            code_at(&params(SHA256_KEY, OtpAlgorithm::Sha256, 8), time),
            sha256,
            "SHA-256 at {time}"
        );
        assert_eq!(
            code_at(&params(SHA512_KEY, OtpAlgorithm::Sha512, 8), time),
            sha512,
            "SHA-512 at {time}"
        );
    }
}

#[test]
fn six_digits_are_the_last_six_of_the_eight() {
    // 94287082 -> 287082 at time 59.
    assert_eq!(
        code_at(&params(SHA1_KEY, OtpAlgorithm::Sha1, 6), 59),
        "287082"
    );
    assert_eq!(
        code_at(&params(SHA1_KEY, OtpAlgorithm::Sha1, 10), 59),
        "1094287082"
    );
}

#[test]
fn base32_is_case_insensitive_and_ignores_spaces_and_padding() {
    let expected = SHA1_KEY.to_vec();
    let encoded = encode_base32(SHA1_KEY);
    assert_eq!(decode_base32(&encoded), Some(expected.clone()));
    assert_eq!(
        decode_base32(&encoded.to_lowercase()),
        Some(expected.clone())
    );
    let spaced: String = encoded
        .chars()
        .enumerate()
        .flat_map(|(i, c)| if i % 4 == 3 { vec![c, ' '] } else { vec![c] })
        .collect();
    assert_eq!(decode_base32(&spaced), Some(expected.clone()));
    assert_eq!(decode_base32(&format!("{encoded}====")), Some(expected));
    assert_eq!(decode_base32("JBSWY3DPEHPK3PXP").map(|b| b.len()), Some(10));
    assert_eq!(decode_base32("not base32 !"), None);
    assert_eq!(decode_base32("1"), None, "1 is not in the alphabet");
}

#[test]
fn a_bare_secret_gets_the_defaults() {
    let p = resolve("JBSWY3DPEHPK3PXP", None, None, None).expect("bare secret");
    assert_eq!((p.digits, p.period), (6, 30));
    assert_eq!(p.algorithm, OtpAlgorithm::Sha1);
    assert_eq!(p.secret.len(), 10);
}

#[test]
fn an_otpauth_address_carries_its_parameters() {
    let p = resolve(
        "otpauth://totp/Example:alice?secret=JBSWY3DPEHPK3PXP&issuer=Example&digits=8&period=60&algorithm=SHA256",
        None,
        None,
        None,
    )
    .expect("otpauth");
    assert_eq!((p.digits, p.period), (8, 60));
    assert_eq!(p.algorithm, OtpAlgorithm::Sha256);
    // Explicit values win over the address.
    let q = resolve(
        "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=8",
        Some(7),
        Some(45),
        Some("sha512"),
    )
    .expect("explicit override");
    assert_eq!((q.digits, q.period), (7, 45));
    assert_eq!(q.algorithm, OtpAlgorithm::Sha512);
    // A percent-escaped label does not break the parse.
    assert!(resolve(
        "otpauth://totp/A%20B?secret=JBSWY3DPEHPK3PXP",
        None,
        None,
        None
    )
    .is_ok());
}

fn reason_of(result: crate::error::Result<OtpParams>) -> String {
    match result {
        Err(crate::error::HaexError::InvalidInput { reason }) => reason,
        other => panic!("expected InvalidInput, got {:?}", other.map(|_| "params")),
    }
}

#[test]
fn invalid_input_is_rejected_with_the_field_name() {
    assert_eq!(
        reason_of(resolve("JBSWY3DPEHPK3PXP", Some(5), None, None)),
        "otpDigits"
    );
    assert_eq!(
        reason_of(resolve("JBSWY3DPEHPK3PXP", Some(11), None, None)),
        "otpDigits"
    );
    assert_eq!(
        reason_of(resolve("JBSWY3DPEHPK3PXP", None, Some(0), None)),
        "otpPeriod"
    );
    assert_eq!(
        reason_of(resolve("JBSWY3DPEHPK3PXP", None, Some(301), None)),
        "otpPeriod"
    );
    assert_eq!(
        reason_of(resolve("JBSWY3DPEHPK3PXP", None, None, Some("MD5"))),
        "otpAlgorithm"
    );
    assert_eq!(
        reason_of(resolve("JBSWY3DPEHPK3PX!", None, None, None)),
        "otpSecret"
    );
    assert_eq!(reason_of(resolve("", None, None, None)), "otpSecret");
    assert_eq!(reason_of(resolve("   ", None, None, None)), "otpSecret");
    assert_eq!(
        reason_of(resolve(
            "otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP",
            None,
            None,
            None
        )),
        "otpSecret",
        "only time based codes"
    );
    assert_eq!(
        reason_of(resolve("otpauth://totp/x?digits=6", None, None, None)),
        "otpSecret",
        "an address without a secret"
    );
    assert_eq!(
        reason_of(resolve(
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=99",
            None,
            None,
            None
        )),
        "otpDigits"
    );
}

#[test]
fn remaining_seconds_counts_down_inside_the_period() {
    assert_eq!(remaining_seconds(30, 0), 30);
    assert_eq!(remaining_seconds(30, 1), 29);
    assert_eq!(remaining_seconds(30, 29), 1);
    assert_eq!(remaining_seconds(30, 30), 30);
    assert_eq!(remaining_seconds(60, 61), 59);
    assert_eq!(
        remaining_seconds(0, 5),
        1,
        "a zero period never divides by zero"
    );
}

#[test]
fn the_state_of_a_stored_value_never_fails() {
    // No secret, or an empty one.
    assert_eq!(otp_state(None, None, None, None), OtpState::None);
    assert_eq!(otp_state(Some(""), None, None, None), OtpState::None);
    assert_eq!(
        otp_state(Some("  "), Some(6), Some(30), Some("SHA1")),
        OtpState::None
    );
    // A valid secret; NULL digits, period and algorithm read as 6, 30 and SHA1.
    assert_eq!(
        otp_state(Some("JBSWY3DPEHPK3PXP"), None, None, None),
        OtpState::Valid
    );
    assert_eq!(
        otp_state(Some("JBSWY3DPEHPK3PXP"), Some(8), Some(60), Some("SHA256")),
        OtpState::Valid
    );
    // What a sync or an import may bring: nothing here may panic.
    for (secret, digits, period, algorithm) in [
        ("JBSWY3DPEHPK3PXP", Some(0), Some(30), Some("SHA1")),
        ("JBSWY3DPEHPK3PXP", Some(-1), Some(30), Some("SHA1")),
        ("JBSWY3DPEHPK3PXP", Some(6), Some(0), Some("SHA1")),
        ("JBSWY3DPEHPK3PXP", Some(6), Some(-30), Some("SHA1")),
        ("JBSWY3DPEHPK3PXP", Some(6), Some(30), Some("WHIRLPOOL")),
        ("JBSWY3DPEHPK3PXP", Some(i64::MAX), Some(30), Some("SHA1")),
        ("not base32 !!", Some(6), Some(30), Some("SHA1")),
        ("otpauth://totp/x?digits=6", None, None, None),
        ("\u{0}\u{1f600}", None, None, None),
    ] {
        assert_eq!(
            otp_state(Some(secret), digits, period, algorithm),
            OtpState::Invalid,
            "{secret:?} {digits:?} {period:?} {algorithm:?}"
        );
    }
}

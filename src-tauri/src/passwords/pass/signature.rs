//! The optional signature check of a `.pkpass` file (spec 092, FR-030, research R6, T067). The
//! result is information for the user and never decides the import: [`check`] returns `valid`,
//! `invalid` or `not_checkable` with a short reason and the name on the pass's certificate, and the
//! caller only shows it.
//!
//! It is purely offline and takes only the bytes of the file (no address in the file or in a
//! certificate is ever called, so a pass cannot make holzi open a connection by being checked), and it
//! never panics. The steps:
//!
//! 1. the files of the pass match the SHA-1 hashes in `manifest.json`, none missing, none extra;
//! 2. the detached CMS signature verifies over the manifest (the signer's signature over the signed
//!    attributes, and the `messageDigest` attribute against the manifest);
//! 3. the signer's certificate chains to one of Apple's roots (embedded from `roots/`), every
//!    signature verified and every validity period checked at the signing time stated in the
//!    signature, not today: a pass outlives the one-year certificate it was signed with.
//!
//! What `valid` says and what it does not: the files are those the signer listed, and the signer's
//! certificate chains to an Apple root. It does not say who the signer is: any certificate Apple's
//! developer programme issues chains there, `organizationName` in the pass is not compared with the
//! certificate, and the certificate is not checked to be a pass type certificate (key usage and
//! the pass type extension are not looked at). The manifest's file hashes are SHA-1, as Apple
//! writes them, so a file could be swapped for a SHA-1 collision of the signed one.
//!
//! Limits worth knowing: the signing time is the signer's own claim (there is no timestamp
//! authority in a pass), so a leaked key could back-date a signature, and revocation cannot be seen
//! offline. A pass that is not signed by Apple (a self-made one) is `not_checkable` with
//! `unknown_root`, not `invalid`, because the signature itself is fine.

use serde::Serialize;
use ts_rs::TS;
use x509_cert::Certificate;

use super::pkpass::SignedParts;
use super::signature_chain::{label, validate, ChainError};
use super::signature_cms::{parse, verify_signer, Parsed};
use super::signature_manifest::check_files;
use der::Decode;

/// Apple's root certificates, the trust anchors. Public data, from `roots/README.md`.
const ROOTS: [&[u8]; 3] = [
    include_bytes!("roots/AppleIncRootCertificate.cer"),
    include_bytes!("roots/AppleRootCA-G2.cer"),
    include_bytes!("roots/AppleRootCA-G3.cer"),
];

/// Apple's Worldwide Developer Relations intermediates, helpers for a signature that does not carry
/// its intermediate. They are never trusted on their own: a chain must end in a root.
const INTERMEDIATES: [&[u8]; 6] = [
    include_bytes!("roots/AppleWWDRCAG1.cer"),
    include_bytes!("roots/AppleWWDRCAG2.cer"),
    include_bytes!("roots/AppleWWDRCAG3.cer"),
    include_bytes!("roots/AppleWWDRCAG4.cer"),
    include_bytes!("roots/AppleWWDRCAG5.cer"),
    include_bytes!("roots/AppleWWDRCAG6.cer"),
];

/// The most certificates a signature may carry: a real one has the pass certificate and its
/// intermediate. More are not looked at (each is a signature to verify).
const MAX_CERTIFICATES: usize = 16;
/// The longest name shown for a certificate.
const MAX_LABEL_CHARS: usize = 100;

/// The three answers of the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum SignatureState {
    /// Files, signature and chain to an Apple root all check out.
    Valid,
    /// Something was changed or does not fit: a file, the manifest, the signature.
    Invalid,
    /// The check cannot say (a signature that is not Apple's, an unreadable one, an algorithm that
    /// is not supported).
    NotCheckable,
}

/// What the check found. `issuer` is the name on the pass's certificate, whatever the state, when
/// it can be read; `reason` is a short code the window turns into a sentence, set unless valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/", optional_fields)]
#[serde(rename_all = "camelCase")]
pub struct SignatureResult {
    pub state: SignatureState,
    pub issuer: Option<String>,
    pub reason: Option<String>,
}

/// Why a check ended before `valid`: the state and the reason code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fail {
    state: SignatureState,
    reason: &'static str,
}

impl Fail {
    pub(super) fn invalid(reason: &'static str) -> Self {
        Self {
            state: SignatureState::Invalid,
            reason,
        }
    }

    pub(super) fn not_checkable(reason: &'static str) -> Self {
        Self {
            state: SignatureState::NotCheckable,
            reason,
        }
    }

    pub(super) fn unreadable() -> Self {
        Self::not_checkable("unreadable")
    }

    pub(super) fn unsupported_algorithm() -> Self {
        Self::not_checkable("unsupported_algorithm")
    }

    pub(super) fn no_signer_certificate() -> Self {
        Self::not_checkable("no_signer_certificate")
    }
}

/// Checks the signature of a pass against Apple's roots.
pub fn check(parts: &SignedParts) -> SignatureResult {
    let roots: Vec<Vec<u8>> = ROOTS.iter().map(|root| root.to_vec()).collect();
    check_with(parts, &roots)
}

/// [`check`] with the trust anchors given (DER certificates), so a test can trust a throwaway root.
/// Whatever happens inside, the answer is a result and never a panic.
pub(super) fn check_with(parts: &SignedParts, anchors: &[Vec<u8>]) -> SignatureResult {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(parts, anchors))).unwrap_or(
        SignatureResult {
            state: SignatureState::NotCheckable,
            issuer: None,
            reason: Some("unreadable".to_owned()),
        },
    )
}

fn run(parts: &SignedParts, anchors: &[Vec<u8>]) -> SignatureResult {
    let parsed = parse(&parts.signature);
    // The name comes from a certificate that is not trusted yet (and may never be): it is shortened
    // and stripped of control characters before it travels to the window.
    let issuer = parsed
        .as_ref()
        .ok()
        .and_then(|p| label(&p.signer))
        .map(|name| {
            name.chars()
                .filter(|c| !c.is_control())
                .take(MAX_LABEL_CHARS)
                .collect::<String>()
        })
        .filter(|name| !name.is_empty());
    let verdict = decide(parts, parsed, anchors);
    match verdict {
        Ok(()) => SignatureResult {
            state: SignatureState::Valid,
            issuer,
            reason: None,
        },
        Err(fail) => SignatureResult {
            state: fail.state,
            issuer,
            reason: Some(fail.reason.to_owned()),
        },
    }
}

fn decode_all(list: &[Vec<u8>]) -> Vec<Certificate> {
    list.iter()
        .filter_map(|der| Certificate::from_der(der).ok())
        .collect()
}

fn decide(
    parts: &SignedParts,
    parsed: Result<Parsed, Fail>,
    anchors: &[Vec<u8>],
) -> Result<(), Fail> {
    // The pass's own files first: a changed file is a finding whatever the signature says. A manifest
    // that is not readable waits for the signature, which may show it was altered.
    let manifest_unreadable = match check_files(&parts.manifest, &parts.files) {
        Ok(()) => false,
        Err(fail) if fail == Fail::unreadable() => true,
        Err(fail) => return Err(fail),
    };
    let parsed = parsed?;
    if parsed.certificates.len() > MAX_CERTIFICATES {
        return Err(Fail::not_checkable("too_many_certificates"));
    }
    let time = verify_signer(&parsed, &parts.manifest)?;
    if manifest_unreadable {
        return Err(Fail::unreadable());
    }
    let time = time.ok_or(Fail::not_checkable("no_signing_time"))?;
    let anchors = decode_all(anchors);
    let mut helpers = parsed.certificates.clone();
    helpers.extend(decode_all(
        &INTERMEDIATES
            .iter()
            .map(|der| der.to_vec())
            .collect::<Vec<_>>(),
    ));
    validate(&parsed.signer, &helpers, &anchors, time).map_err(|error| match error {
        ChainError::Unknown => Fail::not_checkable("unknown_root"),
        ChainError::Unsupported => Fail::unsupported_algorithm(),
        ChainError::Broken => Fail::invalid("chain_broken"),
        // A signature made when the certificate was not valid is not a valid signature.
        ChainError::Expired => Fail::invalid("expired_at_signing"),
        ChainError::TooComplex => Fail::not_checkable("too_many_certificates"),
    })
}

/// The embedded certificates, DER, for the test that all of them parse.
#[cfg(test)]
pub(super) fn embedded() -> (Vec<&'static [u8]>, Vec<&'static [u8]>) {
    (ROOTS.to_vec(), INTERMEDIATES.to_vec())
}

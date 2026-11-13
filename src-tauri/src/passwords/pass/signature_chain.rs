//! The certificate chain of the pkpass signature check (spec 092, T067): from the signer's
//! certificate up to one of the trust anchors (Apple's roots), every signature verified, every
//! issuer a CA, every validity period checked at the time the pass was signed and not today
//! (passes outlive their one-year certificates). Offline: certificates come from the signature, the
//! embedded Apple intermediates and the anchors, never from an address in a certificate.
//!
//! This is a small chain builder for the shape of Apple's chain, not a general RFC 5280 validator:
//! it does not look at name constraints, policies, key usage or path length, and it cannot see
//! revocation (that needs the network).

use x509_cert::ext::pkix::BasicConstraints;
use x509_cert::Certificate;

use super::signature_algs::{verify_certificate, VerifyError};

/// The longest chain looked at: leaf, intermediates, root.
const MAX_DEPTH: usize = 6;

/// Why no trusted chain was found, from the least to the most telling: the worst one found wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ChainError {
    /// No certificate with the issuer's name is known: the chain ends somewhere unknown.
    Unknown,
    /// A certificate on the way is signed with an algorithm that is not supported.
    Unsupported,
    /// An issuer with the right name is known, but the signature does not verify or it is no CA.
    Broken,
    /// The chain is there, but a certificate was not valid at the signing time.
    Expired,
}

/// Whether `cert` was valid at `at` (seconds since the Unix epoch).
pub(super) fn valid_at(cert: &Certificate, at: u64) -> bool {
    let validity = cert.tbs_certificate().validity();
    validity.not_before.to_unix_duration().as_secs() <= at
        && at <= validity.not_after.to_unix_duration().as_secs()
}

fn is_ca(cert: &Certificate) -> bool {
    matches!(
        cert.tbs_certificate().get_extension::<BasicConstraints>(),
        Ok(Some((_, constraints))) if constraints.ca
    )
}

/// Checks that `leaf` chains to one of `anchors` through `helpers`, as it was valid at `at`.
pub(super) fn validate(
    leaf: &Certificate,
    helpers: &[Certificate],
    anchors: &[Certificate],
    at: u64,
) -> Result<(), ChainError> {
    if !valid_at(leaf, at) {
        return Err(ChainError::Expired);
    }
    climb(leaf, helpers, anchors, at, MAX_DEPTH)
}

fn climb(
    current: &Certificate,
    helpers: &[Certificate],
    anchors: &[Certificate],
    at: u64,
    depth: usize,
) -> Result<(), ChainError> {
    if anchors.iter().any(|anchor| anchor == current) {
        return Ok(());
    }
    if depth == 0 {
        return Err(ChainError::Unknown);
    }
    let wanted = current.tbs_certificate().issuer();
    let candidates = anchors
        .iter()
        .map(|anchor| (anchor, true))
        .chain(helpers.iter().map(|helper| (helper, false)));
    let mut worst = ChainError::Unknown;
    for (candidate, is_anchor) in candidates {
        if candidate == current || candidate.tbs_certificate().subject() != wanted {
            continue;
        }
        match verify_certificate(
            candidate.tbs_certificate().subject_public_key_info(),
            current,
        ) {
            Ok(()) => {}
            Err(VerifyError::Unsupported) => {
                worst = worst.max(ChainError::Unsupported);
                continue;
            }
            Err(VerifyError::Mismatch) => {
                worst = worst.max(ChainError::Broken);
                continue;
            }
        }
        if !is_ca(candidate) {
            worst = worst.max(ChainError::Broken);
            continue;
        }
        if !valid_at(candidate, at) {
            worst = worst.max(ChainError::Expired);
            continue;
        }
        if is_anchor {
            return Ok(());
        }
        if let Err(error) = climb(candidate, helpers, anchors, at, depth - 1) {
            worst = worst.max(error);
        } else {
            return Ok(());
        }
    }
    Err(worst)
}

/// The name shown for a certificate: its organisation, or its common name when it has none.
pub(super) fn label(cert: &Certificate) -> Option<String> {
    let subject = cert.tbs_certificate().subject();
    let text = |value: Option<x509_cert::ext::pkix::name::DirectoryString>| {
        value
            .map(|text| text.value().trim().to_owned())
            .filter(|text| !text.is_empty())
    };
    text(subject.organization().ok().flatten())
        .or_else(|| text(subject.common_name().ok().flatten()))
}

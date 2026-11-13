//! The detached PKCS#7 / CMS signature of a pass (spec 092, T067): reads the `SignedData`, finds the
//! signer's certificate in its certificate set, and verifies the signer's signature over the signed
//! attributes and their `messageDigest` over the manifest (RFC 5652 section 5.4 and 5.6). Where the
//! certificates lead to is `signature_chain`'s business.

use cms::cert::CertificateChoices;
use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerIdentifier, SignerInfo};
use der::asn1::{ObjectIdentifier, OctetString};
use der::{Decode, Encode, EncodingRules};
use x509_cert::ext::pkix::SubjectKeyIdentifier;
use x509_cert::time::Time;
use x509_cert::Certificate;

use super::signature::Fail;
use super::signature_algs::{verify, Hash, VerifyError};

const SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const SIGNING_TIME: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");

/// A signature that was read: the signer, its certificate and every certificate that came with it.
pub(super) struct Parsed {
    signer_info: SignerInfo,
    pub(super) signer: Certificate,
    pub(super) certificates: Vec<Certificate>,
}

/// Reads the signature. Both DER and BER are accepted (a tool may write an indefinite length).
pub(super) fn parse(bytes: &[u8]) -> Result<Parsed, Fail> {
    let info = ContentInfo::from_ber(bytes).map_err(|_| Fail::unreadable())?;
    if info.content_type != SIGNED_DATA {
        return Err(Fail::unreadable());
    }
    let data = info
        .content
        .decode_as_encoding::<SignedData>(EncodingRules::Ber)
        .map_err(|_| Fail::unreadable())?;
    let certificates: Vec<Certificate> = data
        .certificates
        .map(|set| {
            set.0
                .into_vec()
                .into_iter()
                .filter_map(|choice| match choice {
                    CertificateChoices::Certificate(cert) => Some(cert),
                    CertificateChoices::Other(_) => None,
                })
                .collect()
        })
        .unwrap_or_default();
    // A pass has one signer; the first signer whose certificate is in the set is the one checked.
    for signer_info in data.signer_infos.0.into_vec() {
        if let Some(signer) = certificates
            .iter()
            .find(|cert| is_signer(cert, &signer_info.sid))
        {
            let signer = signer.clone();
            return Ok(Parsed {
                signer_info,
                signer,
                certificates,
            });
        }
    }
    Err(Fail::no_signer_certificate())
}

fn is_signer(cert: &Certificate, sid: &SignerIdentifier) -> bool {
    let tbs = cert.tbs_certificate();
    match sid {
        SignerIdentifier::IssuerAndSerialNumber(wanted) => {
            tbs.issuer() == &wanted.issuer && tbs.serial_number() == &wanted.serial_number
        }
        SignerIdentifier::SubjectKeyIdentifier(wanted) => matches!(
            tbs.get_extension::<SubjectKeyIdentifier>(),
            Ok(Some((_, have))) if &have == wanted
        ),
    }
}

/// Verifies the signer's signature and that the signature belongs to `manifest`. Returns the
/// signing time (seconds since the Unix epoch) when the signature states one.
pub(super) fn verify_signer(parsed: &Parsed, manifest: &[u8]) -> Result<Option<u64>, Fail> {
    let info = &parsed.signer_info;
    let hash = Hash::from_oid(&info.digest_alg.oid).ok_or_else(Fail::unsupported_algorithm)?;
    let key = parsed.signer.tbs_certificate().subject_public_key_info();
    let Some(attributes) = &info.signed_attrs else {
        // No signed attributes: the signature is over the manifest itself, and no time is stated.
        check(
            verify(
                key,
                &info.signature_algorithm,
                Some(hash),
                manifest,
                info.signature.as_bytes(),
            ),
            "bad_signature",
        )?;
        return Ok(None);
    };
    // The signature covers the attributes as a DER SET, not with the context tag they travel with.
    let signed = attributes.to_der().map_err(|_| Fail::unreadable())?;
    check(
        verify(
            key,
            &info.signature_algorithm,
            Some(hash),
            &signed,
            info.signature.as_bytes(),
        ),
        "bad_signature",
    )?;
    let digest = attributes
        .iter()
        .find(|attribute| attribute.oid == MESSAGE_DIGEST)
        .and_then(|attribute| attribute.values.iter().next())
        .and_then(|value| value.decode_as::<OctetString>().ok())
        .ok_or_else(Fail::unreadable)?;
    if digest.as_bytes() != hash.digest(manifest).as_slice() {
        return Err(Fail::invalid("digest_mismatch"));
    }
    let time = attributes
        .iter()
        .find(|attribute| attribute.oid == SIGNING_TIME)
        .and_then(|attribute| attribute.values.iter().next())
        .and_then(|value| value.to_der().ok())
        .and_then(|der| Time::from_der(&der).ok())
        .map(|time| time.to_unix_duration().as_secs());
    Ok(time)
}

fn check(outcome: Result<(), VerifyError>, reason: &'static str) -> Result<(), Fail> {
    outcome.map_err(|error| match error {
        VerifyError::Mismatch => Fail::invalid(reason),
        VerifyError::Unsupported => Fail::unsupported_algorithm(),
    })
}

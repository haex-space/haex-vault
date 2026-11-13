//! The signature and digest algorithms the pkpass signature check can verify (spec 092, T067):
//! RSA PKCS#1 v1.5 and ECDSA over P-256 and P-384, with SHA-1, SHA-256, SHA-384 and SHA-512. A pass
//! chain has the pass certificate (RSA) under an Apple intermediate (RSA or ECDSA P-384) under an
//! Apple root (RSA or ECDSA P-384), and old certificates are signed with SHA-1, so all of these are
//! needed to read real files. Everything here is pure Rust and takes only bytes.

use der::asn1::ObjectIdentifier;
use der::Encode;
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::signature::Verifier;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha384, Sha512};
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};

const RSA_ENCRYPTION: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const SHA1_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.5");
const SHA256_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const SHA384_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.12");
const SHA512_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.13");
const EC_PUBLIC_KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");
const ECDSA_WITH_SHA1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.1");
const ECDSA_WITH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const ECDSA_WITH_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.3");
const ECDSA_WITH_SHA512: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.4");
const CURVE_P256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.3.1.7");
const CURVE_P384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.132.0.34");
const HASH_SHA1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.14.3.2.26");
const HASH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const HASH_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.2");
const HASH_SHA512: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.3");

/// A digest the check knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hash {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    pub(super) fn from_oid(oid: &ObjectIdentifier) -> Option<Self> {
        match *oid {
            HASH_SHA1 => Some(Self::Sha1),
            HASH_SHA256 => Some(Self::Sha256),
            HASH_SHA384 => Some(Self::Sha384),
            HASH_SHA512 => Some(Self::Sha512),
            _ => None,
        }
    }

    pub(super) fn digest(self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha1 => Sha1::digest(data).to_vec(),
            Self::Sha256 => Sha256::digest(data).to_vec(),
            Self::Sha384 => Sha384::digest(data).to_vec(),
            Self::Sha512 => Sha512::digest(data).to_vec(),
        }
    }
}

/// Why a signature did not verify: it is wrong, or it uses something this build cannot check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VerifyError {
    Mismatch,
    Unsupported,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Rsa,
    Ecdsa,
}

/// The family of a signature algorithm and the digest it names. `rsaEncryption` and a bare ECDSA
/// key name no digest (the signer's own digest algorithm applies, RFC 5652 section 5.3).
fn classify(algorithm: &ObjectIdentifier) -> Option<(Family, Option<Hash>)> {
    Some(match *algorithm {
        RSA_ENCRYPTION => (Family::Rsa, None),
        SHA1_WITH_RSA => (Family::Rsa, Some(Hash::Sha1)),
        SHA256_WITH_RSA => (Family::Rsa, Some(Hash::Sha256)),
        SHA384_WITH_RSA => (Family::Rsa, Some(Hash::Sha384)),
        SHA512_WITH_RSA => (Family::Rsa, Some(Hash::Sha512)),
        EC_PUBLIC_KEY => (Family::Ecdsa, None),
        ECDSA_WITH_SHA1 => (Family::Ecdsa, Some(Hash::Sha1)),
        ECDSA_WITH_SHA256 => (Family::Ecdsa, Some(Hash::Sha256)),
        ECDSA_WITH_SHA384 => (Family::Ecdsa, Some(Hash::Sha384)),
        ECDSA_WITH_SHA512 => (Family::Ecdsa, Some(Hash::Sha512)),
        _ => return None,
    })
}

/// Verifies `signature` over `message` with the public key in `key`. `algorithm` is the signature
/// algorithm of the certificate or signer; `fallback` is the digest to use when it names none.
pub(super) fn verify(
    key: &SubjectPublicKeyInfoOwned,
    algorithm: &AlgorithmIdentifierOwned,
    fallback: Option<Hash>,
    message: &[u8],
    signature: &[u8],
) -> Result<(), VerifyError> {
    let (family, named) = classify(&algorithm.oid).ok_or(VerifyError::Unsupported)?;
    let hash = named.or(fallback).ok_or(VerifyError::Unsupported)?;
    let key_bytes = key.subject_public_key.raw_bytes();
    match (family, key.algorithm.oid) {
        (Family::Rsa, RSA_ENCRYPTION) => verify_rsa(key_bytes, hash, message, signature),
        (Family::Ecdsa, EC_PUBLIC_KEY) => {
            let curve = key
                .algorithm
                .parameters
                .as_ref()
                .and_then(|any| any.decode_as::<ObjectIdentifier>().ok())
                .ok_or(VerifyError::Unsupported)?;
            let prehash = hash.digest(message);
            match curve {
                CURVE_P256 => verify_p256(key_bytes, &prehash, signature),
                CURVE_P384 => verify_p384(key_bytes, &prehash, signature),
                _ => Err(VerifyError::Unsupported),
            }
        }
        _ => Err(VerifyError::Unsupported),
    }
}

fn verify_rsa(
    key_bytes: &[u8],
    hash: Hash,
    message: &[u8],
    signature: &[u8],
) -> Result<(), VerifyError> {
    let key = rsa::RsaPublicKey::from_pkcs1_der(key_bytes).map_err(|_| VerifyError::Unsupported)?;
    let signature =
        rsa::pkcs1v15::Signature::try_from(signature).map_err(|_| VerifyError::Mismatch)?;
    let outcome = match hash {
        Hash::Sha1 => rsa::pkcs1v15::VerifyingKey::<Sha1>::new(key).verify(message, &signature),
        Hash::Sha256 => rsa::pkcs1v15::VerifyingKey::<Sha256>::new(key).verify(message, &signature),
        Hash::Sha384 => rsa::pkcs1v15::VerifyingKey::<Sha384>::new(key).verify(message, &signature),
        Hash::Sha512 => rsa::pkcs1v15::VerifyingKey::<Sha512>::new(key).verify(message, &signature),
    };
    outcome.map_err(|_| VerifyError::Mismatch)
}

fn verify_p256(key_bytes: &[u8], prehash: &[u8], signature: &[u8]) -> Result<(), VerifyError> {
    use p256::ecdsa::signature::hazmat::PrehashVerifier;
    let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(key_bytes)
        .map_err(|_| VerifyError::Unsupported)?;
    let signature =
        p256::ecdsa::DerSignature::from_bytes(signature).map_err(|_| VerifyError::Mismatch)?;
    key.verify_prehash(prehash, &signature)
        .map_err(|_| VerifyError::Mismatch)
}

fn verify_p384(key_bytes: &[u8], prehash: &[u8], signature: &[u8]) -> Result<(), VerifyError> {
    use p384::ecdsa::signature::hazmat::PrehashVerifier;
    let key = p384::ecdsa::VerifyingKey::from_sec1_bytes(key_bytes)
        .map_err(|_| VerifyError::Unsupported)?;
    let signature =
        p384::ecdsa::DerSignature::from_bytes(signature).map_err(|_| VerifyError::Mismatch)?;
    key.verify_prehash(prehash, &signature)
        .map_err(|_| VerifyError::Mismatch)
}

/// Verifies the signature of a certificate with the public key of its issuer.
pub(super) fn verify_certificate(
    issuer_key: &SubjectPublicKeyInfoOwned,
    cert: &x509_cert::Certificate,
) -> Result<(), VerifyError> {
    // The signed bytes are the certificate's TBS part; DER re-encoding gives back the original
    // bytes of a certificate that is DER, which Apple's are (the embedded ones are tested).
    let tbs = cert
        .tbs_certificate()
        .to_der()
        .map_err(|_| VerifyError::Unsupported)?;
    verify(
        issuer_key,
        cert.signature_algorithm(),
        None,
        &tbs,
        cert.signature().raw_bytes(),
    )
}

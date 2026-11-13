//! Throwaway chains and signed passes for the tests of the signature check (spec 092, T064): a
//! root, an intermediate and a pass certificate with keys made here, a manifest signed into a
//! detached CMS signature. No Apple certificate and no real pass is used; the check is told to trust
//! the throwaway root through `check_with`.

use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, UNIX_EPOCH};

use cms::cert::CertificateChoices;
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{
    CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos,
};
use der::asn1::{ObjectIdentifier, OctetString, SetOfVec};
use der::{Any, Decode, Encode};
use sha1::{Digest as _, Sha1};
use sha2::Sha256;
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};
use x509_cert::attr::Attribute;
use x509_cert::builder::profile::BuilderProfile;
use x509_cert::builder::{Builder, CertificateBuilder};
use x509_cert::certificate::TbsCertificate;
use x509_cert::ext::pkix::BasicConstraints;
use x509_cert::ext::{Extension, ToExtension};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::{Time, Validity};
use x509_cert::Certificate;

use super::pkpass::SignedParts;

/// 2020-01-01, 2020-06-01 and 2021-01-01 (UTC), as seconds since the Unix epoch.
pub(super) const Y2020: u64 = 1_577_836_800;
pub(super) const SIGNED_AT: u64 = 1_590_969_600;
pub(super) const Y2021: u64 = 1_609_459_200;
/// Well past `SIGNED_AT` and before the end of the intermediate and the root.
pub(super) const LATER: u64 = 1_893_456_000;

const SIGNING_TIME: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");
const ID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
const ECDSA_WITH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const SHA256_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
const SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");

/// A throwaway key of one of the kinds a pass chain uses.
pub(super) enum Key {
    P256(p256::ecdsa::SigningKey),
    P384(p384::ecdsa::SigningKey),
    Rsa(rsa::pkcs1v15::SigningKey<Sha256>),
}

impl Key {
    /// A fixed key: the same seed gives the same key, so the tests need no randomness.
    pub(super) fn p256(seed: u8) -> Self {
        Self::P256(p256::ecdsa::SigningKey::from_slice(&[seed; 32]).expect("p256 key"))
    }

    pub(super) fn p384(seed: u8) -> Self {
        Self::P384(p384::ecdsa::SigningKey::from_slice(&[seed; 48]).expect("p384 key"))
    }

    pub(super) fn rsa() -> Self {
        let mut rng = getrandom::rand_core::UnwrapErr(getrandom::SysRng);
        let key = rsa::RsaPrivateKey::new(&mut rng, 2048).expect("rsa key");
        Self::Rsa(rsa::pkcs1v15::SigningKey::new(key))
    }

    fn public(&self) -> SubjectPublicKeyInfoOwned {
        match self {
            Self::P256(key) => SubjectPublicKeyInfoOwned::from_key(key.verifying_key()),
            Self::P384(key) => SubjectPublicKeyInfoOwned::from_key(key.verifying_key()),
            Self::Rsa(key) => {
                SubjectPublicKeyInfoOwned::from_key(&rsa::signature::Keypair::verifying_key(key))
            }
        }
        .expect("public key")
    }

    fn issue(&self, builder: CertificateBuilder<Plain>) -> Certificate {
        match self {
            Self::P256(key) => builder.build::<_, p256::ecdsa::DerSignature>(key),
            Self::P384(key) => builder.build::<_, p384::ecdsa::DerSignature>(key),
            Self::Rsa(key) => builder.build::<_, rsa::pkcs1v15::Signature>(key),
        }
        .expect("certificate")
    }

    /// The signature algorithm a CMS signer info names for this key (SHA-256, as the tests digest).
    fn signature_algorithm(&self) -> AlgorithmIdentifierOwned {
        let (oid, parameters) = match self {
            Self::P256(_) => (ECDSA_WITH_SHA256, None),
            Self::Rsa(_) => (SHA256_WITH_RSA, Some(Any::null())),
            Self::P384(_) => unreachable!("the tests sign passes with P-256 or RSA"),
        };
        AlgorithmIdentifierOwned { oid, parameters }
    }

    fn sign_bytes(&self, message: &[u8]) -> Vec<u8> {
        use rsa::signature::{SignatureEncoding, Signer};
        match self {
            Self::P256(key) => Signer::<p256::ecdsa::DerSignature>::sign(key, message).to_vec(),
            Self::P384(key) => Signer::<p384::ecdsa::DerSignature>::sign(key, message).to_vec(),
            Self::Rsa(key) => Signer::<rsa::pkcs1v15::Signature>::sign(key, message).to_vec(),
        }
    }
}

/// The only profile the tests need: a name for the subject, one for the issuer, CA or not.
struct Plain {
    subject: Name,
    issuer: Name,
    ca: bool,
}

impl BuilderProfile for Plain {
    fn get_issuer(&self, _subject: &Name) -> Name {
        self.issuer.clone()
    }

    fn get_subject(&self) -> Name {
        self.subject.clone()
    }

    fn build_extensions(
        &self,
        _spk: spki::SubjectPublicKeyInfoRef<'_>,
        _issuer_spk: spki::SubjectPublicKeyInfoRef<'_>,
        tbs: &TbsCertificate,
    ) -> x509_cert::builder::Result<Vec<Extension>> {
        let constraints = BasicConstraints {
            ca: self.ca,
            path_len_constraint: None,
        };
        Ok(vec![constraints.to_extension(tbs.subject(), &[])?])
    }
}

pub(super) fn time(seconds: u64) -> Time {
    Time::try_from(UNIX_EPOCH + Duration::from_secs(seconds)).expect("time")
}

fn name(text: &str) -> Name {
    Name::from_str(text).expect("name")
}

/// A certificate for `subject_key` issued by `issuer_key` (the same key for a root).
pub(super) fn certificate(
    subject: &str,
    subject_key: &Key,
    issuer: &str,
    issuer_key: &Key,
    ca: bool,
    valid: (u64, u64),
) -> Certificate {
    let profile = Plain {
        subject: name(subject),
        issuer: name(issuer),
        ca,
    };
    let builder = CertificateBuilder::new(
        profile,
        SerialNumber::from(7u32),
        Validity::new(time(valid.0), time(valid.1)),
        subject_key.public(),
    )
    .expect("builder");
    issuer_key.issue(builder)
}

/// A root, an intermediate under it and a pass certificate under that, with their keys.
pub(super) struct Chain {
    pub(super) root: Certificate,
    pub(super) intermediate: Certificate,
    pub(super) leaf: Certificate,
    pub(super) leaf_key: Key,
}

pub(super) const ROOT_NAME: &str = "CN=Throwaway Root,O=Throwaway Authority";
pub(super) const INTERMEDIATE_NAME: &str = "CN=Throwaway Intermediate,O=Throwaway Authority";
pub(super) const LEAF_NAME: &str = "CN=Pass Type ID: pass.test.throwaway,O=Throwaway Pass Org";

/// The shape of a real chain: ECDSA P-256 root, ECDSA P-384 intermediate, pass certificate of the
/// given key, valid for 2020 (so expired today, as passes outlive their certificates).
pub(super) fn chain(root_seed: u8, leaf_key: Key) -> Chain {
    let root_key = Key::p256(root_seed);
    let intermediate_key = Key::p384(root_seed);
    let forever = (Y2020, LATER + Y2020);
    let root = certificate(ROOT_NAME, &root_key, ROOT_NAME, &root_key, true, forever);
    let intermediate = certificate(
        INTERMEDIATE_NAME,
        &intermediate_key,
        ROOT_NAME,
        &root_key,
        true,
        forever,
    );
    let leaf = certificate(
        LEAF_NAME,
        &leaf_key,
        INTERMEDIATE_NAME,
        &intermediate_key,
        false,
        (Y2020, Y2021),
    );
    Chain {
        root,
        intermediate,
        leaf,
        leaf_key,
    }
}

/// What goes into the signature besides the manifest.
#[derive(Clone, Copy)]
pub(super) struct Options {
    /// The signing time attribute; `None` leaves it out.
    pub(super) signing_time: Option<u64>,
    /// Whether the intermediate travels in the signature's certificate set.
    pub(super) with_intermediate: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            signing_time: Some(SIGNED_AT),
            with_intermediate: true,
        }
    }
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The manifest of the files: name to SHA-1 in hex, as Apple writes it.
pub(super) fn manifest(files: &HashMap<String, Vec<u8>>) -> Vec<u8> {
    let listed: std::collections::BTreeMap<_, _> = files
        .iter()
        .map(|(name, data)| (name.clone(), hex(&Sha1::digest(data))))
        .collect();
    serde_json::to_vec(&listed).expect("manifest")
}

fn attribute(oid: ObjectIdentifier, value_der: &[u8]) -> Attribute {
    let mut values = SetOfVec::new();
    values
        .insert(Any::from_der(value_der).expect("any"))
        .expect("set");
    Attribute { oid, values }
}

/// A detached CMS signature (DER) over `manifest`, signed by the chain's pass certificate. It is
/// assembled by hand from the `cms` types, the way openssl or Apple's tools lay it out: signed
/// attributes (content type, message digest, signing time) signed as a DER SET.
pub(super) fn sign(chain: &Chain, manifest: &[u8], options: Options) -> Vec<u8> {
    let digest_algorithm = AlgorithmIdentifierOwned {
        oid: SHA256,
        parameters: None,
    };
    let digest = Sha256::digest(manifest);
    let mut attributes = vec![
        attribute(CONTENT_TYPE, &ID_DATA.to_der().expect("oid")),
        attribute(
            MESSAGE_DIGEST,
            &OctetString::new(digest.as_slice())
                .expect("digest")
                .to_der()
                .expect("der"),
        ),
    ];
    if let Some(seconds) = options.signing_time {
        attributes.push(attribute(
            SIGNING_TIME,
            &time(seconds).to_der().expect("time"),
        ));
    }
    let signed_attrs = SetOfVec::try_from(attributes).expect("attributes");
    let signed = signed_attrs.to_der().expect("attributes der");
    let signature = chain.leaf_key.sign_bytes(&signed);
    let info = SignerInfo {
        version: CmsVersion::V1,
        sid: SignerIdentifier::from(&chain.leaf),
        digest_alg: digest_algorithm.clone(),
        signed_attrs: Some(signed_attrs),
        signature_algorithm: chain.leaf_key.signature_algorithm(),
        signature: OctetString::new(signature).expect("signature"),
        unsigned_attrs: None,
    };
    let mut certificates = vec![CertificateChoices::Certificate(chain.leaf.clone())];
    if options.with_intermediate {
        certificates.push(CertificateChoices::Certificate(chain.intermediate.clone()));
    }
    let data = SignedData {
        version: CmsVersion::V1,
        digest_algorithms: SetOfVec::try_from(vec![digest_algorithm]).expect("algorithms"),
        encap_content_info: EncapsulatedContentInfo {
            econtent_type: ID_DATA,
            econtent: None,
        },
        certificates: Some(CertificateSet::try_from(certificates).expect("certificates")),
        crls: None,
        signer_infos: SignerInfos::try_from(vec![info]).expect("signers"),
    };
    ContentInfo {
        content_type: SIGNED_DATA,
        content: Any::encode_from(&data).expect("content"),
    }
    .to_der()
    .expect("der")
}

/// A small pass with a few files and the signed parts of it.
pub(super) fn signed_parts(chain: &Chain, options: Options) -> SignedParts {
    let mut files = HashMap::new();
    files.insert("pass.json".to_owned(), br#"{"formatVersion":1}"#.to_vec());
    files.insert("icon.png".to_owned(), b"\x89PNG throwaway".to_vec());
    files.insert(
        "de.lproj/pass.strings".to_owned(),
        b"\"a\" = \"b\";".to_vec(),
    );
    let manifest = manifest(&files);
    let signature = sign(chain, &manifest, options);
    SignedParts {
        manifest,
        signature,
        files,
    }
}

pub(super) fn der_of(cert: &Certificate) -> Vec<u8> {
    cert.to_der().expect("der")
}

//! Tests of the optional pkpass signature check (spec 092, T064): throwaway roots and keys only, no
//! real pass and no Apple-signed file in the repository. The embedded Apple certificates are tested
//! as data (they parse, they chain to each other) because a bad download must not ship.

use der::Decode;
use x509_cert::Certificate;

use super::pkpass::SignedParts;
use super::signature::{self, check_with, SignatureResult, SignatureState};
use super::signature_chain::{valid_at, validate, ChainError};
use super::signature_fixtures::{
    certificate, chain, der_of, manifest, sign, signed_parts, time, Chain, Key, Options,
    INTERMEDIATE_NAME, LATER, LEAF_NAME, ROOT_NAME, SIGNED_AT, Y2020, Y2021,
};

fn anchors(chain: &Chain) -> Vec<Vec<u8>> {
    vec![der_of(&chain.root)]
}

fn ec_chain() -> Chain {
    chain(1, Key::p256(9))
}

fn reason(result: &SignatureResult) -> Option<&str> {
    result.reason.as_deref()
}

#[test]
fn a_pass_signed_under_a_trusted_root_is_valid_with_the_issuer_name() {
    let chain = ec_chain();
    let parts = signed_parts(&chain, Options::default());
    let result = check_with(&parts, &anchors(&chain));
    assert_eq!(result.state, SignatureState::Valid, "{result:?}");
    assert_eq!(result.issuer.as_deref(), Some("Throwaway Pass Org"));
    assert_eq!(result.reason, None);
}

#[test]
fn an_rsa_pass_certificate_is_valid_like_the_real_ones() {
    let chain = chain(2, Key::rsa());
    let parts = signed_parts(&chain, Options::default());
    let result = check_with(&parts, &anchors(&chain));
    assert_eq!(result.state, SignatureState::Valid, "{result:?}");
}

#[test]
fn the_pass_certificate_may_have_expired_after_the_signing_time() {
    let chain = ec_chain();
    let leaf = &chain.leaf.tbs_certificate().validity().not_after;
    assert!(
        leaf.to_unix_duration().as_secs()
            < std::time::UNIX_EPOCH.elapsed().expect("clock").as_secs(),
        "the throwaway pass certificate is expired today"
    );
    let parts = signed_parts(&chain, Options::default());
    assert_eq!(
        check_with(&parts, &anchors(&chain)).state,
        SignatureState::Valid
    );
}

#[test]
fn the_intermediate_need_not_travel_in_the_signature_when_it_is_known_otherwise() {
    // Without the intermediate in the signature and not embedded, the chain ends unknown; with the
    // intermediate trusted as an anchor it is complete again.
    let chain = ec_chain();
    let options = Options {
        with_intermediate: false,
        ..Options::default()
    };
    let parts = signed_parts(&chain, options);
    let result = check_with(&parts, &anchors(&chain));
    assert_eq!(result.state, SignatureState::NotCheckable);
    assert_eq!(reason(&result), Some("unknown_root"));
    let both = vec![der_of(&chain.root), der_of(&chain.intermediate)];
    assert_eq!(check_with(&parts, &both).state, SignatureState::Valid);
}

#[test]
fn a_changed_missing_or_extra_file_is_invalid() {
    let chain = ec_chain();
    let anchors = anchors(&chain);

    let mut changed = signed_parts(&chain, Options::default());
    changed
        .files
        .insert("icon.png".to_owned(), b"other".to_vec());
    let result = check_with(&changed, &anchors);
    assert_eq!(result.state, SignatureState::Invalid);
    assert_eq!(reason(&result), Some("file_changed"));
    // The name is shown even when the result is bad.
    assert_eq!(result.issuer.as_deref(), Some("Throwaway Pass Org"));

    let mut missing = signed_parts(&chain, Options::default());
    missing.files.remove("de.lproj/pass.strings");
    let result = check_with(&missing, &anchors);
    assert_eq!(result.state, SignatureState::Invalid);
    assert_eq!(reason(&result), Some("file_missing"));

    let mut extra = signed_parts(&chain, Options::default());
    extra.files.insert("extra.png".to_owned(), b"new".to_vec());
    let result = check_with(&extra, &anchors);
    assert_eq!(result.state, SignatureState::Invalid);
    assert_eq!(reason(&result), Some("file_not_in_manifest"));
}

#[test]
fn a_flipped_bit_in_the_signature_is_invalid() {
    let chain = ec_chain();
    let mut parts = signed_parts(&chain, Options::default());
    // The signature value is the last element of the structure.
    *parts.signature.last_mut().expect("signature") ^= 0x01;
    let result = check_with(&parts, &anchors(&chain));
    assert_eq!(result.state, SignatureState::Invalid, "{result:?}");
    assert_eq!(reason(&result), Some("bad_signature"));
}

#[test]
fn a_changed_manifest_is_invalid_whether_or_not_it_still_reads() {
    let chain = ec_chain();
    let anchors = anchors(&chain);

    // A hash in the manifest changed: the file no longer matches it.
    let mut hash = signed_parts(&chain, Options::default());
    let at = hash.manifest.len() - 5;
    hash.manifest[at] = if hash.manifest[at] == b'0' {
        b'1'
    } else {
        b'0'
    };
    assert_eq!(check_with(&hash, &anchors).state, SignatureState::Invalid);

    // Only the bytes changed (white space): every file matches, the signature does not.
    let mut spaced = signed_parts(&chain, Options::default());
    spaced.manifest.push(b' ');
    let result = check_with(&spaced, &anchors);
    assert_eq!(result.state, SignatureState::Invalid);
    assert_eq!(reason(&result), Some("digest_mismatch"));

    // Not JSON any more: still invalid, because the signature shows the bytes are not the signed ones.
    let mut broken = signed_parts(&chain, Options::default());
    broken.manifest[0] ^= 0x01;
    let result = check_with(&broken, &anchors);
    assert_eq!(result.state, SignatureState::Invalid);
    assert_eq!(reason(&result), Some("digest_mismatch"));
}

#[test]
fn a_chain_to_an_unknown_root_is_not_checkable_but_names_the_issuer() {
    let chain = ec_chain();
    let parts = signed_parts(&chain, Options::default());
    // Trusting a different root, one with another name: the signature is fine, the root is not one
    // we know.
    let other_key = Key::p256(3);
    let other_name = "CN=Other Root,O=Other Authority";
    let other = certificate(
        other_name,
        &other_key,
        other_name,
        &other_key,
        true,
        (Y2020, LATER),
    );
    let result = check_with(&parts, &[der_of(&other)]);
    assert_eq!(result.state, SignatureState::NotCheckable);
    assert_eq!(reason(&result), Some("unknown_root"));
    assert_eq!(result.issuer.as_deref(), Some("Throwaway Pass Org"));
    // And with the embedded Apple roots, which is what `check` does, a self-made pass is the same.
    let result = signature::check(&parts);
    assert_eq!(result.state, SignatureState::NotCheckable);
    assert_eq!(reason(&result), Some("unknown_root"));
}

#[test]
fn a_chain_that_claims_a_known_issuer_but_is_signed_by_another_key_is_broken() {
    // The same names as the trusted chain, other keys: what a forger who copies the names has.
    let trusted = chain(1, Key::p256(9));
    let rogue_root_key = Key::p256(5);
    let rogue_inter_key = Key::p384(5);
    let forever = (Y2020, LATER + Y2020);
    let rogue_intermediate = certificate(
        INTERMEDIATE_NAME,
        &rogue_inter_key,
        ROOT_NAME,
        &rogue_root_key,
        true,
        forever,
    );
    let leaf_key = Key::p256(9);
    let leaf = certificate(
        LEAF_NAME,
        &leaf_key,
        INTERMEDIATE_NAME,
        &rogue_inter_key,
        false,
        (Y2020, Y2021),
    );
    let forged = Chain {
        root: trusted.root.clone(),
        intermediate: rogue_intermediate,
        leaf,
        leaf_key,
    };
    let parts = signed_parts(&forged, Options::default());
    let result = check_with(&parts, &anchors(&trusted));
    assert_eq!(result.state, SignatureState::Invalid, "{result:?}");
    assert_eq!(reason(&result), Some("chain_broken"));
}

#[test]
fn a_signature_made_outside_the_certificates_validity_is_invalid() {
    // Decision: a signature made when the pass certificate was not valid is `invalid` with
    // `expired_at_signing`, not `not_checkable`; the signature itself is fine, but nobody could have
    // made it with a valid certificate at that time. Passes are judged at the signing time, so a
    // certificate that expired later is fine (tested above) and one not yet valid is not.
    let chain = ec_chain();
    for seconds in [Y2020 - 1, Y2021 + 1, LATER] {
        let options = Options {
            signing_time: Some(seconds),
            ..Options::default()
        };
        let parts = signed_parts(&chain, options);
        let result = check_with(&parts, &anchors(&chain));
        assert_eq!(
            result.state,
            SignatureState::Invalid,
            "{seconds}: {result:?}"
        );
        assert_eq!(reason(&result), Some("expired_at_signing"));
    }
}

#[test]
fn a_signature_without_a_signing_time_is_not_checkable() {
    let chain = ec_chain();
    let options = Options {
        signing_time: None,
        ..Options::default()
    };
    let parts = signed_parts(&chain, options);
    let result = check_with(&parts, &anchors(&chain));
    assert_eq!(result.state, SignatureState::NotCheckable);
    assert_eq!(reason(&result), Some("no_signing_time"));
    assert_eq!(result.issuer.as_deref(), Some("Throwaway Pass Org"));
}

#[test]
fn garbage_is_not_checkable_and_never_a_panic() {
    let chain = ec_chain();
    let good = signed_parts(&chain, Options::default());
    let anchors = anchors(&chain);

    for garbage in [
        &b""[..],
        b"not a signature",
        &[0u8; 64],
        &[0x30, 0x80, 0x00, 0x00],
    ] {
        let parts = SignedParts {
            signature: garbage.to_vec(),
            ..good.clone()
        };
        let result = check_with(&parts, &anchors);
        assert_eq!(result.state, SignatureState::NotCheckable);
        assert_eq!(reason(&result), Some("unreadable"));
        assert_eq!(result.issuer, None);
    }

    // Every prefix of a real signature, and a manifest that is not JSON, give an answer and no panic.
    for length in (0..good.signature.len()).step_by(5) {
        let parts = SignedParts {
            signature: good.signature[..length].to_vec(),
            ..good.clone()
        };
        assert_ne!(check_with(&parts, &anchors).state, SignatureState::Valid);
    }
    let parts = SignedParts {
        manifest: b"[1, 2]".to_vec(),
        ..good
    };
    assert_ne!(check_with(&parts, &anchors).state, SignatureState::Valid);
}

#[test]
fn the_check_for_a_manifest_that_is_not_an_object_waits_for_the_signature() {
    // A signed manifest that is not a JSON object is not checkable, and nothing is claimed about files.
    let chain = ec_chain();
    let files = std::collections::HashMap::new();
    let bytes = b"[1, 2]".to_vec();
    let parts = SignedParts {
        signature: sign(&chain, &bytes, Options::default()),
        manifest: bytes,
        files,
    };
    let result = check_with(&parts, &anchors(&chain));
    assert_eq!(result.state, SignatureState::NotCheckable);
    assert_eq!(reason(&result), Some("unreadable"));
}

#[test]
fn the_validity_period_is_judged_at_the_given_time() {
    let chain = ec_chain();
    let roots = [chain.root.clone()];
    let helpers = [chain.intermediate.clone()];
    assert_eq!(validate(&chain.leaf, &helpers, &roots, SIGNED_AT), Ok(()));
    assert_eq!(
        validate(&chain.leaf, &helpers, &roots, Y2021 + 10),
        Err(ChainError::Expired)
    );
    assert!(valid_at(&chain.leaf, SIGNED_AT));
    assert!(!valid_at(&chain.leaf, Y2021 + 10));
    // A time helper that agrees with the fixtures.
    assert_eq!(time(SIGNED_AT).to_unix_duration().as_secs(), SIGNED_AT);
}

#[test]
fn a_manifest_names_its_files_by_sha1() {
    let chain = ec_chain();
    let parts = signed_parts(&chain, Options::default());
    assert_eq!(parts.manifest, manifest(&parts.files));
}

#[test]
fn the_embedded_apple_certificates_parse_and_chain_to_the_embedded_roots() {
    let (roots, intermediates) = signature::embedded();
    assert_eq!(roots.len(), 3);
    assert_eq!(intermediates.len(), 6);
    let roots: Vec<Certificate> = roots
        .iter()
        .map(|der| Certificate::from_der(der).expect("a root parses as DER X.509"))
        .collect();
    let intermediates: Vec<Certificate> = intermediates
        .iter()
        .map(|der| Certificate::from_der(der).expect("an intermediate parses as DER X.509"))
        .collect();
    // 2022-06-01: every one of them was valid then, the first generation of intermediates included.
    let at = 1_654_041_600;
    for root in &roots {
        // A root is its own issuer: the anchor itself is accepted, and its self-signature verifies.
        assert_eq!(validate(root, &[], &roots, at), Ok(()));
        assert_eq!(
            super::signature_algs::verify_certificate(
                root.tbs_certificate().subject_public_key_info(),
                root
            ),
            Ok(()),
            "a root's own signature verifies (RSA and ECDSA P-384 are both covered)"
        );
    }
    for intermediate in &intermediates {
        assert_eq!(
            validate(intermediate, &[], &roots, at),
            Ok(()),
            "{:?} chains to an embedded root",
            intermediate.tbs_certificate().subject().to_string()
        );
    }
    // Today the first generation has expired and the others have not: the time is what is judged.
    assert!(!valid_at(&intermediates[0], 1_893_456_000));
    assert!(valid_at(&intermediates[5], 1_893_456_000));
}

#[test]
fn the_check_takes_only_bytes_and_uses_no_network() {
    // `check` takes the parts of a file and nothing else; the modules that make it up do not name
    // anything that opens a connection. A guard against a later change that adds one.
    let sources = [
        include_str!("signature.rs"),
        include_str!("signature_algs.rs"),
        include_str!("signature_chain.rs"),
        include_str!("signature_cms.rs"),
        include_str!("signature_manifest.rs"),
    ];
    for source in sources {
        for forbidden in [
            "reqwest",
            "std::net",
            "tokio::net",
            "TcpStream",
            "UdpSocket",
            "hyper",
            "ureq",
            "://",
        ] {
            assert!(!source.contains(forbidden), "found {forbidden}");
        }
    }
    let _: fn(&SignedParts) -> SignatureResult = signature::check;
}

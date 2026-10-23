use super::*;

const SECRET: [u8; 32] = [7; 32];

#[test]
fn a_signature_verifies_in_its_own_domain_only() {
    let pubkey = xonly_public_key(&SECRET).expect("public key");
    let signature = sign(Domain::DeviceList, b"payload", &SECRET).expect("sign");

    verify(Domain::DeviceList, b"payload", &signature, &pubkey).expect("verifies");
    assert!(matches!(
        verify(Domain::KeyGeneration, b"payload", &signature, &pubkey),
        Err(SigningError::BadSignature)
    ));
}

/// Spec 057 research R4: a proof of the sync handshake never stands in for one of a channel.
#[test]
fn a_device_auth_signature_does_not_verify_as_ext_peer_auth() {
    let pubkey = xonly_public_key(&SECRET).expect("public key");
    let signature = sign(Domain::DeviceAuth, b"transcript", &SECRET).expect("sign");

    verify(Domain::DeviceAuth, b"transcript", &signature, &pubkey).expect("verifies");
    assert!(matches!(
        verify(Domain::ExtPeerAuth, b"transcript", &signature, &pubkey),
        Err(SigningError::BadSignature)
    ));
}

/// Spec 056 research R4: a proof for extension channels never opens the files of a device.
#[test]
fn an_ext_peer_signature_does_not_verify_as_files_auth() {
    let pubkey = xonly_public_key(&SECRET).expect("public key");
    let signature = sign(Domain::ExtPeerAuth, b"transcript", &SECRET).expect("sign");

    assert!(matches!(
        verify(Domain::FilesAuth, b"transcript", &signature, &pubkey),
        Err(SigningError::BadSignature)
    ));
}

#[test]
fn a_changed_payload_or_key_fails() {
    let pubkey = xonly_public_key(&SECRET).expect("public key");
    let other = xonly_public_key(&[8; 32]).expect("other key");
    let signature = sign(Domain::Admission, b"payload", &SECRET).expect("sign");

    assert!(verify(Domain::Admission, b"payloaD", &signature, &pubkey).is_err());
    assert!(verify(Domain::Admission, b"payload", &signature, &other).is_err());
}

#[test]
fn the_zero_scalar_is_no_secret_key() {
    assert!(matches!(
        sign(Domain::Link, b"x", &[0; 32]),
        Err(SigningError::InvalidSecretKey)
    ));
}

#[test]
fn every_domain_has_a_distinct_versioned_tag() {
    let domains = [
        Domain::DeviceAuth,
        Domain::DeviceList,
        Domain::KeyGeneration,
        Domain::KeyEnvelope,
        Domain::Admission,
        Domain::Link,
        Domain::LinkResume,
        Domain::ContactCard,
        Domain::ExtPeerAuth,
        Domain::FilesAuth,
    ];
    let tags: std::collections::HashSet<_> = domains.iter().map(|d| d.tag()).collect();
    assert_eq!(tags.len(), domains.len());
    assert!(domains.iter().all(|d| d.tag().ends_with(b"/v1")));
}

#[test]
fn the_digest_is_sha256_of_tag_then_bytes() {
    let expected: [u8; 32] = Sha256::digest(b"holzi-device-auth/v1abc").into();
    assert_eq!(digest(Domain::DeviceAuth, b"abc"), expected);
}

#[test]
fn lp_prefixes_the_big_endian_length() {
    assert_eq!(lp(b"ab"), vec![0, 0, 0, 2, b'a', b'b']);
    assert_eq!(lp(b""), vec![0, 0, 0, 0]);
}

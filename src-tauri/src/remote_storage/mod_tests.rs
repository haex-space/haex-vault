//! The types of the storages across protocols (spec 050 data-model.md): protocols and the providers
//! that fit them, the location of a storage, and credentials that never show their secrets.

use zeroize::Zeroizing;

use super::{
    Addressing, ConnectionRow, Credentials, EndpointScope, Location, PasswordCredentials, Protocol,
    ProviderKind, S3Credentials, SshKeyCredentials,
};

#[test]
fn protocols_read_back_and_know_their_providers() {
    for protocol in [Protocol::S3, Protocol::WebDav, Protocol::Sftp] {
        assert_eq!(Protocol::parse(protocol.as_str()), Some(protocol));
    }
    assert_eq!(Protocol::WebDav.as_str(), "webdav");
    assert_eq!(Protocol::parse("ftp"), None);
    assert!(!Protocol::S3.is_tree());
    assert!(Protocol::WebDav.is_tree() && Protocol::Sftp.is_tree());
    assert!(Protocol::S3.fits(ProviderKind::Rustfs));
    assert!(!Protocol::S3.fits(ProviderKind::Nextcloud));
    assert!(Protocol::WebDav.fits(ProviderKind::Koofr));
    assert!(!Protocol::WebDav.fits(ProviderKind::Aws));
    assert!(Protocol::Sftp.fits(ProviderKind::HetznerStorageBox));
    assert!(!Protocol::Sftp.fits(ProviderKind::Kdrive));
}

#[test]
fn the_location_of_a_storage_carries_protocol_and_host_key() {
    let connection = ConnectionRow {
        id: "c".to_owned(),
        protocol: Protocol::Sftp,
        provider_name: "Box".to_owned(),
        provider_kind: ProviderKind::HetznerStorageBox,
        endpoint: "sftp://box:23".to_owned(),
        endpoint_scope: EndpointScope::Public,
        region: String::new(),
        addressing: Addressing::Path,
        host_key: Some("ssh-ed25519 AAAA".to_owned()),
        host_key_confirmed_at: Some("t".to_owned()),
        credentials_item_id: "item".to_owned(),
        created_at: "t".to_owned(),
        updated_at: "t".to_owned(),
    };
    let location = Location::of(&connection, "Fotos");
    assert_eq!(location.protocol, Protocol::Sftp);
    assert_eq!(location.host_key.as_deref(), Some("ssh-ed25519 AAAA"));
    assert_eq!(location.bucket, "Fotos");
    assert!(location.is_tree());
}

#[test]
fn credentials_never_show_their_secrets() {
    let all = [
        Credentials::S3(S3Credentials {
            access_key_id: "AKID".to_owned(),
            secret_access_key: Zeroizing::new("secret-one".to_owned()),
            session_token: Some(Zeroizing::new("secret-two".to_owned())),
        }),
        Credentials::Password(PasswordCredentials {
            username: "u123".to_owned(),
            password: Zeroizing::new("secret-three".to_owned()),
        }),
        Credentials::SshKey(SshKeyCredentials {
            username: "u123".to_owned(),
            private_key: Zeroizing::new("secret-four".to_owned()),
            passphrase: Some(Zeroizing::new("secret-five".to_owned())),
        }),
    ];
    for credentials in all {
        let shown = format!("{credentials:?}");
        assert!(!shown.contains("secret-"), "{shown}");
        assert!(shown.contains("AKID") || shown.contains("u123"), "{shown}");
    }
}

#[test]
fn a_local_folder_is_never_stored_nor_taken_from_the_window() {
    assert_eq!(Protocol::Local.as_str(), "local");
    assert_eq!(Protocol::parse("local"), None);
    assert!(Protocol::Local.is_tree());
    assert!(!Protocol::Local.fits(ProviderKind::Other));
    assert!(serde_json::from_str::<Protocol>("\"local\"").is_err());
    assert!(serde_json::to_string(&Protocol::Local).is_err());
    assert_eq!(
        serde_json::from_str::<Protocol>("\"sftp\"").ok(),
        Some(Protocol::Sftp)
    );
}

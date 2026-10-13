//! SFTP connections in the settings (spec 050 US4, research R4, R5, T076): a new connection ends
//! its test at the unknown host key and names it, saving with the confirmed key keeps exactly
//! that key, a changed key stops every call until the user takes the key the server showed, and
//! a generated key reaches the password manager while its private half never left Rust.

use std::sync::Arc;

use zeroize::Zeroizing;

use super::model::{ConnectionInput, CredentialsInput, StorageInput};
use super::protocol::ProtocolStore;
use super::service::StorageService;
use super::sftp::keys;
use super::sftp::test_server::{self, attempts, Options, TestServer};
use super::sftp::SftpStore;
use super::test_support::{resolver, vault, FakeStore};
use super::{store, Addressing, Protocol, ProviderKind, TestOutcome};
use crate::error::HolziError;
use crate::passwords::service::PasswordsService;
use crate::vault_gate::VaultDb;

struct Scene {
    _dir: tempfile::TempDir,
    db: VaultDb,
    service: StorageService,
    server: TestServer,
}

async fn scene(options: Options) -> Scene {
    let server = test_server::start(options).await;
    std::fs::create_dir(server.path("Ablage")).expect("folder");
    let (dir, db) = vault();
    let store = Arc::new(ProtocolStore::new(
        Arc::new(FakeStore::new()),
        None,
        Some(Arc::new(SftpStore::new(resolver(&[])))),
    ));
    let service = StorageService::new(
        db.clone(),
        PasswordsService::new(db.clone()),
        store,
        resolver(&[]),
    );
    Scene {
        _dir: dir,
        db,
        service,
        server,
    }
}

fn sftp(endpoint: &str, credentials: CredentialsInput, host_key: Option<&str>) -> ConnectionInput {
    ConnectionInput {
        id: None,
        protocol: Protocol::Sftp,
        provider_name: "NAS".to_owned(),
        provider_kind: ProviderKind::Other,
        endpoint: Some(endpoint.to_owned()),
        region: String::new(),
        addressing: Addressing::Path,
        credentials: Some(credentials),
        bucket_for_test: "Ablage".to_owned(),
        create_folder: false,
        host_key: host_key.map(str::to_owned),
    }
}

fn login() -> CredentialsInput {
    CredentialsInput::Password {
        username: "anna".to_owned(),
        password: Zeroizing::new("password".to_owned()),
    }
}

async fn connections(db: &VaultDb) -> usize {
    db.read(|q| Ok(store::connections(q)?.len()))
        .await
        .expect("count")
}

#[tokio::test]
async fn a_new_connection_stops_at_the_unknown_host_key_and_saves_with_the_confirmed_one() {
    let s = scene(Options::new(21)).await;
    let refused = s
        .service
        .save_connection(sftp(&s.server.endpoint(), login(), None))
        .await
        .unwrap_err();
    let HolziError::StorageTestFailed {
        outcome, host_key, ..
    } = refused
    else {
        panic!("unexpected {refused:?}");
    };
    assert_eq!(outcome, TestOutcome::HostKeyUnknown);
    let shown = host_key.expect("the shown key");
    assert_eq!(shown.key, s.server.host_key);
    assert_eq!(
        Some(shown.fingerprint.clone()),
        keys::fingerprint(&s.server.host_key)
    );
    assert_eq!(connections(&s.db).await, 0, "nothing saved");
    assert_eq!(
        attempts(&s.server),
        0,
        "no login before the key is confirmed"
    );

    let saved = s
        .service
        .save_connection(sftp(&s.server.endpoint(), login(), Some(&shown.key)))
        .await
        .expect("saved with the confirmed key");
    let confirmed = saved.host_key.expect("confirmed key");
    assert_eq!(confirmed.fingerprint, shown.fingerprint);
    assert!(!confirmed.confirmed_at.is_empty());
}

#[tokio::test]
async fn a_changed_key_stops_calls_until_the_user_takes_the_shown_one() {
    let s = scene(Options::new(22)).await;
    let saved = s
        .service
        .save_connection(sftp(
            &s.server.endpoint(),
            login(),
            Some(&s.server.host_key),
        ))
        .await
        .expect("saved");
    let storage = s
        .service
        .save_storage(StorageInput {
            id: None,
            connection_id: saved.id.clone(),
            name: "Ablage".to_owned(),
            bucket: "Ablage".to_owned(),
            create_folder: false,
        })
        .await
        .expect("storage");
    // The server's key changes: the confirmed one is now another.
    let other = test_server::start(Options::new(23)).await;
    let mut row = s.service.connection(&saved.id).await.expect("row");
    row.host_key = Some(other.host_key.clone());
    s.db.write(move |tx| Ok(store::put_connection(tx, &row)?))
        .await
        .expect("write");
    let before = attempts(&s.server);
    let tested = s.service.test_storage(&storage.id).await.expect("tested");
    assert_eq!(tested.outcome, TestOutcome::HostKeyChanged);
    let shown = tested.host_key.expect("the shown key");
    assert_eq!(shown.key, s.server.host_key);
    assert_eq!(
        attempts(&s.server),
        before,
        "SC-004: no login with a changed key"
    );

    let wrong = s.service.accept_host_key(&saved.id, &other.host_key).await;
    assert!(
        matches!(wrong, Err(HolziError::StorageInvalid { ref field }) if field == "hostKey"),
        "only the key the server showed: {wrong:?}"
    );
    let taken = s
        .service
        .accept_host_key(&saved.id, &shown.key)
        .await
        .expect("taken");
    assert_eq!(taken.host_key.expect("key").fingerprint, shown.fingerprint);
    let tested = s.service.test_storage(&storage.id).await.expect("tested");
    assert_eq!(tested.outcome, TestOutcome::Passed);
}

#[tokio::test]
async fn a_generated_key_signs_in_and_leaves_the_process_once_saved() {
    let (key_id, public) = keys::pending().generate().expect("generate");
    let mut options = Options::new(24);
    options.password = None;
    options.public_key = Some(keys::parse_host_key(&public).expect("public key"));
    let s = scene(options).await;
    let generated = || CredentialsInput::GeneratedKey {
        username: "anna".to_owned(),
        key_id: key_id.clone(),
    };
    let first = s
        .service
        .save_connection(sftp(&s.server.endpoint(), generated(), None))
        .await;
    assert!(
        matches!(
            first,
            Err(HolziError::StorageTestFailed {
                outcome: TestOutcome::HostKeyUnknown,
                ..
            })
        ),
        "{first:?}"
    );
    assert!(
        keys::pending().get(&key_id).is_some(),
        "a failed save keeps the key for the next try"
    );
    s.service
        .save_connection(sftp(
            &s.server.endpoint(),
            generated(),
            Some(&s.server.host_key),
        ))
        .await
        .expect("saved");
    assert!(
        keys::pending().get(&key_id).is_none(),
        "FR-008: gone from the process"
    );
    let unknown = s
        .service
        .save_connection(sftp(
            &s.server.endpoint(),
            CredentialsInput::GeneratedKey {
                username: "anna".to_owned(),
                key_id: "unbekannt".to_owned(),
            },
            Some(&s.server.host_key),
        ))
        .await;
    assert!(
        matches!(unknown, Err(HolziError::StorageInvalid { ref field }) if field == "credentials"),
        "{unknown:?}"
    );
}

//! An extension proposes a WebDAV storage (spec 050 US6, T056, contracts/bridge.md): the `add`
//! permission names host and port of the address, credentials of every kind are refused before any
//! dialog, `sameProviderAs` keeps to one protocol, a missing folder is made only on the user's
//! word in holzi's window, and a change of a WebDAV storage knows `folder`, not `bucket`.

use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::extensions::remote_storage_dialog::{StorageAnswer, StorageTrial};
use crate::extensions::remote_storage_test_support::{
    setup, WebdavServer, WEBDAV_PASSWORD, WEBDAV_USER,
};
use crate::remote_storage::model::CredentialsInput;
use crate::remote_storage::TestOutcome;

fn answer(credentials: bool, create_folder: bool) -> StorageAnswer {
    StorageAnswer::Confirm {
        connection_id: None,
        credentials: credentials.then(|| CredentialsInput::Password {
            username: WEBDAV_USER.to_owned(),
            password: Zeroizing::new(WEBDAV_PASSWORD.to_owned()),
        }),
        name: None,
        bucket: None,
        create_folder,
        host_key: None,
    }
}

fn webdav(url: &str, folder: &str) -> Value {
    json!({ "request": { "name": "Ablage", "type": "webdav", "config": { "url": url, "folder": folder } } })
}

/// `host:port` of a server of the test process.
fn host_port(server: &WebdavServer) -> String {
    server
        .endpoint
        .trim_start_matches("http://")
        .split('/')
        .next()
        .expect("host and port")
        .to_owned()
}

#[test]
fn a_webdav_proposal_asks_for_its_host_and_makes_a_missing_folder_only_on_the_users_word() {
    let s = setup();
    let server = WebdavServer::start("");
    let proposal = webdav(server.endpoint.trim_end_matches('/'), "Neu");
    let error = s
        .call("extension_remote_storage_add_backend", proposal.clone())
        .unwrap_err();
    assert_eq!(error.code.as_u16(), 1004);
    assert_eq!(
        error.details.expect("details")["target"],
        json!(host_port(&server))
    );
    assert!(s.events.dialogs().is_empty(), "asked before any dialog");

    s.permit("add", &host_port(&server), "granted");
    s.events
        .answer_in_turn(vec![answer(true, false), answer(true, true)]);
    let added = s
        .call("extension_remote_storage_add_backend", proposal)
        .expect("added");
    assert_eq!(
        s.events.trials(),
        [
            Some(StorageTrial::Failed {
                outcome: TestOutcome::FolderMissing,
                leftover_key: None,
                host_key: None,
            }),
            Some(StorageTrial::Ended),
        ],
        "the missing folder goes back to holzi's window, which says to make it"
    );
    let dialog = s.events.dialogs().pop().expect("a dialog");
    assert_eq!(dialog["proposal"]["protocol"], json!("webdav"));
    assert_eq!(dialog["proposal"]["bucket"], json!("Neu"));
    assert_eq!(dialog["proposal"]["scope"], json!("local"));
    assert_eq!(
        dialog["proposal"]["endpoint"],
        json!(server.endpoint),
        "with its /"
    );
    assert_eq!(added["type"], json!("webdav"));
    assert_eq!(added["folder"], json!("Neu"));
    assert!(added.get("bucket").is_none());
    let id = added["id"].as_str().expect("id");
    let upload = json!({ "request": { "backendId": id, "key": "k.txt", "data": "aGk=" } });
    s.call("extension_remote_storage_upload", upload)
        .expect("the extension may write its new storage");
}

#[test]
fn credentials_of_every_kind_are_refused_before_any_dialog() {
    let s = setup();
    s.permit("add", "*", "granted");
    for field in [
        "username",
        "password",
        "privateKey",
        "passphrase",
        "hostKey",
        "fingerprint",
    ] {
        let mut in_config = webdav("https://dav.example.com/", "x");
        in_config["request"]["config"][field] = json!("x");
        let mut in_request = webdav("https://dav.example.com/", "x");
        in_request["request"][field] = json!("x");
        for call in [in_config, in_request] {
            let error = s
                .call("extension_remote_storage_add_backend", call)
                .unwrap_err();
            assert_eq!(error.code.as_u16(), 3001, "{field}");
            assert_eq!(error.message, "credentials are entered in holzi");
        }
    }
    assert!(s.events.dialogs().is_empty(), "no dialog appeared");
}

#[test]
fn the_add_permission_names_the_port_of_the_scheme() {
    let s = setup();
    let error = s
        .call(
            "extension_remote_storage_add_backend",
            webdav("https://Dav.Example.com/remote.php/dav/files/anna", "x"),
        )
        .unwrap_err();
    assert_eq!(
        error.details.expect("details")["target"],
        json!("dav.example.com:443")
    );
}

#[test]
fn the_same_provider_means_the_same_protocol() {
    let s = setup();
    let server = WebdavServer::start("Ablage");
    let dav = s.add_webdav(&server, "Server", "Ablage");
    s.permit("read", &s.photos, "granted");
    s.permit("read", &dav, "granted");
    let on = |same: &str, kind: &str, config: Value| json!({ "request": { "name": "Neu", "type": kind, "sameProviderAs": same, "config": config } });
    assert_eq!(
        s.code(
            "extension_remote_storage_add_backend",
            on(&s.photos, "webdav", json!({ "folder": "x" }))
        ),
        3001,
        "an S3 storage is no WebDAV provider"
    );
    assert_eq!(
        s.code(
            "extension_remote_storage_add_backend",
            on(&dav, "webdav", json!({ "bucket": "x" }))
        ),
        3001,
        "a WebDAV storage has a folder"
    );
    s.events.answer_with(answer(false, true));
    let added = s
        .call(
            "extension_remote_storage_add_backend",
            on(&dav, "webdav", json!({ "folder": "Zweit" })),
        )
        .expect("added");
    assert_eq!(added["folder"], json!("Zweit"));
    assert_eq!(added["providerName"], json!("Server"));
}

#[test]
fn a_change_of_a_webdav_storage_knows_its_folder() {
    let s = setup();
    let server = WebdavServer::start("Ablage");
    let dav = s.add_webdav(&server, "Server", "Ablage");
    s.permit("readWrite", &dav, "granted");
    let change = |config: Value| json!({ "request": { "backendId": dav, "config": config } });
    assert_eq!(
        s.code(
            "extension_remote_storage_update_backend",
            change(json!({ "bucket": "x" }))
        ),
        3001
    );
    assert_eq!(
        s.code(
            "extension_remote_storage_update_backend",
            change(json!({ "folder": "Neu" }))
        ),
        1002,
        "cancelled"
    );
    s.events.answer_with(answer(false, false));
    assert_eq!(
        s.call(
            "extension_remote_storage_update_backend",
            change(json!({ "folder": "Neu" }))
        )
        .unwrap_err()
        .details
        .expect("details")["kind"],
        json!("folderMissing"),
        "without the user's word no folder is made"
    );
    s.events.answer_with(answer(false, true));
    let changed = s
        .call(
            "extension_remote_storage_update_backend",
            change(json!({ "folder": "Neu" })),
        )
        .expect("changed");
    assert_eq!(changed["folder"], json!("Neu"));
    let dialog = s.events.dialogs().pop().expect("a dialog");
    assert_eq!(dialog["proposal"]["protocol"], json!("webdav"));
}

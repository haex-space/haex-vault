//! An extension proposes an SFTP storage (spec 050 US6, T086, contracts/bridge.md): `host`, `port`
//! (22 unless given) and `folder`; the `add` permission names `host:port`; the server's host key
//! comes back to holzi's window, which saves only after the user confirmed exactly that key, and
//! declining there is 1002. A host key or a fingerprint in a proposal is refused before any dialog.

use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::extensions::remote_storage_dialog::{StorageAnswer, StorageTrial};
use crate::extensions::remote_storage_test_support::{
    setup, SftpServer, WEBDAV_PASSWORD, WEBDAV_USER,
};
use crate::remote_storage::model::CredentialsInput;
use crate::remote_storage::sftp::keys::fingerprint;
use crate::remote_storage::TestOutcome;

fn login(host_key: Option<&str>) -> StorageAnswer {
    StorageAnswer::Confirm {
        connection_id: None,
        credentials: Some(CredentialsInput::Password {
            username: WEBDAV_USER.to_owned(),
            password: Zeroizing::new(WEBDAV_PASSWORD.to_owned()),
        }),
        name: None,
        bucket: None,
        create_folder: false,
        host_key: host_key.map(str::to_owned),
    }
}

fn sftp(config: Value) -> Value {
    json!({ "request": { "name": "Ablage", "type": "sftp", "config": config } })
}

#[test]
fn an_sftp_proposal_saves_only_after_the_user_confirmed_the_shown_host_key() {
    let s = setup();
    let server = SftpServer::start("Ablage");
    let address = server.server.address;
    let proposal = sftp(json!({
        "host": address.ip().to_string(),
        "port": address.port(),
        "folder": "Ablage",
    }));
    let error = s
        .call("extension_remote_storage_add_backend", proposal.clone())
        .unwrap_err();
    assert_eq!(error.code.as_u16(), 1004);
    assert_eq!(
        error.details.expect("details")["target"],
        json!(address.to_string())
    );
    s.permit("add", &address.to_string(), "granted");

    let shown = server.server.host_key.clone();
    s.events
        .answer_in_turn(vec![login(None), login(Some(&shown))]);
    let added = s
        .call("extension_remote_storage_add_backend", proposal)
        .expect("added");
    let trials = s.events.trials();
    let Some(Some(StorageTrial::Failed {
        outcome, host_key, ..
    })) = trials.first()
    else {
        panic!("{trials:?}");
    };
    assert_eq!(*outcome, TestOutcome::HostKeyUnknown);
    let host_key = host_key.as_ref().expect("the shown key");
    assert_eq!(host_key.key, shown);
    assert_eq!(Some(host_key.fingerprint.clone()), fingerprint(&shown));
    assert_eq!(trials.get(1), Some(&Some(StorageTrial::Ended)));
    assert_eq!(added["type"], json!("sftp"));
    assert_eq!(added["folder"], json!("Ablage"));
    let dialog = s.events.dialogs().pop().expect("a dialog");
    assert_eq!(dialog["proposal"]["protocol"], json!("sftp"));
    assert!(
        !dialog.to_string().contains(&shown),
        "the dialog over the tab never shows the key"
    );
}

#[test]
fn declining_the_host_key_cancels_and_saves_nothing() {
    let s = setup();
    let server = SftpServer::start("Ablage");
    let address = server.server.address;
    s.permit("add", &address.to_string(), "granted");
    s.events
        .answer_in_turn(vec![login(None), StorageAnswer::Cancel]);
    let error = s
        .call(
            "extension_remote_storage_add_backend",
            sftp(json!({
                "host": address.ip().to_string(),
                "port": address.port(),
                "folder": "Ablage",
            })),
        )
        .unwrap_err();
    assert_eq!(error.code.as_u16(), 1002);
    assert_eq!(
        s.count("SELECT COUNT(*) FROM haex_storage_connections WHERE protocol = 'sftp'"),
        0
    );
}

#[test]
fn port_22_unless_given_and_no_host_key_in_a_proposal() {
    let s = setup();
    let error = s
        .call(
            "extension_remote_storage_add_backend",
            sftp(json!({ "host": "Storage.Example.com", "folder": "x" })),
        )
        .unwrap_err();
    assert_eq!(
        error.details.expect("details")["target"],
        json!("storage.example.com:22")
    );
    s.permit("add", "*", "granted");
    for field in ["hostKey", "fingerprint"] {
        let mut proposal = sftp(json!({ "host": "storage.example.com", "folder": "x" }));
        proposal["request"]["config"][field] = json!("ssh-ed25519 AAAA");
        assert_eq!(
            s.code("extension_remote_storage_add_backend", proposal),
            3001,
            "{field}"
        );
    }
    for bad in [
        json!({ "host": "sftp://x", "folder": "" }),
        json!({ "host": "x", "port": 0, "folder": "" }),
    ] {
        assert_eq!(
            s.code("extension_remote_storage_add_backend", sftp(bad.clone())),
            3001,
            "{bad}"
        );
    }
    assert!(s.events.dialogs().is_empty(), "no dialog appeared");
}

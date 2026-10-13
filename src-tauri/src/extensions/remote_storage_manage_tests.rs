//! An extension proposes, changes, tests and removes storages (spec 038 US3, T038, T039): only
//! through a confirmed dialog of holzi, credentials never through the bridge, a proposed endpoint
//! only with the `add` permission for its host. Credentials whose test fails:
//! `remote_storage_credentials_tests.rs`.

use std::io;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::extensions::remote_storage_dialog::StorageAnswer;
use crate::extensions::remote_storage_test_support::{
    credentials, setup, setup_with, ACCESS_KEY, ENDPOINT, REGION, SECRET,
};
use crate::remote_storage::address::Resolver;
use crate::remote_storage::test_support::Op;
use crate::remote_storage::StorageError;
use crate::storage::query::Query;

fn confirm() -> StorageAnswer {
    StorageAnswer::Confirm {
        connection_id: None,
        credentials: None,
        name: None,
        bucket: None,
        create_folder: false,
    }
}

fn with_credentials() -> StorageAnswer {
    StorageAnswer::Confirm {
        connection_id: None,
        credentials: Some(credentials()),
        name: None,
        bucket: None,
        create_folder: false,
    }
}

fn add(config: Value) -> Value {
    json!({ "request": { "name": "Neu", "type": "s3", "config": config } })
}

fn same_provider(storage: &str, config: Value) -> Value {
    json!({ "request": { "name": "Neu", "type": "s3", "sameProviderAs": storage, "config": config } })
}

#[test]
fn credentials_in_a_call_are_refused_before_any_dialog() {
    let s = setup();
    s.permit("add", "*", "granted");
    for field in ["accessKeyId", "secretAccessKey", "sessionToken"] {
        let mut config =
            json!({ "endpoint": "https://s3.example.com", "region": "eu", "bucket": "b" });
        config[field] = json!("x");
        assert_eq!(
            s.code("extension_remote_storage_add_backend", add(config)),
            3001
        );
        let update = json!({ "request": { "backendId": s.photos, "config": { field: "x" } } });
        assert_eq!(
            s.code("extension_remote_storage_update_backend", update),
            3001
        );
    }
    assert!(s.events.dialogs().is_empty(), "no dialog appeared");
}

#[test]
fn a_bucket_on_the_same_provider_needs_read_on_that_storage_and_a_confirmation() {
    let s = setup();
    assert_eq!(
        s.code(
            "extension_remote_storage_add_backend",
            same_provider(&s.photos, json!({ "bucket": "fresh" }))
        ),
        1004
    );
    s.permit("read", &s.photos, "granted");
    assert_eq!(
        s.code(
            "extension_remote_storage_add_backend",
            same_provider(&s.photos, json!({ "bucket": "fresh", "region": "x" }))
        ),
        3001,
        "endpoint, region and addressing come from the connection"
    );
    assert!(s.events.dialogs().is_empty());

    assert_eq!(
        s.code(
            "extension_remote_storage_add_backend",
            same_provider(&s.photos, json!({ "bucket": "fresh" }))
        ),
        1002,
        "cancelled"
    );
    assert_eq!(
        s.count("SELECT COUNT(*) FROM haex_storages"),
        2,
        "nothing created"
    );

    s.events.answer_with(confirm());
    let added = s
        .call(
            "extension_remote_storage_add_backend",
            same_provider(&s.photos, json!({ "bucket": "fresh" })),
        )
        .expect("added");
    assert_eq!(added["name"], json!("Neu"));
    assert_eq!(added["bucket"], json!("fresh"));
    assert_eq!(added["providerName"], json!("RustFS"));
    let id = added["id"].as_str().expect("id").to_owned();
    let upload = json!({ "request": { "backendId": id, "key": "k", "data": "aGk=" } });
    s.call("extension_remote_storage_upload", upload)
        .expect("the extension may read and write its new storage");
    let dialog = s.events.dialogs().pop().expect("a dialog");
    assert_eq!(dialog["kind"], json!("add"));
    assert!(!dialog.to_string().contains(SECRET));
}

#[test]
fn a_proposed_local_endpoint_needs_the_add_permission_for_its_host() {
    let s = setup();
    let local = add(json!({ "endpoint": ENDPOINT, "region": "home", "bucket": "nas" }));
    let error = s
        .call("extension_remote_storage_add_backend", local.clone())
        .unwrap_err();
    assert_eq!(error.code.as_u16(), 1004);
    assert_eq!(
        error.details,
        Some(
            json!({ "resourceType": "remoteStorage", "action": "add", "target": "127.0.0.1:9000" })
        )
    );
    assert!(s.events.dialogs().is_empty(), "asked before any dialog");

    s.permit("add", "127.0.0.1:9000", "granted");
    s.events.answer_with(with_credentials());
    let added = s
        .call("extension_remote_storage_add_backend", local)
        .expect("added");
    let dialog = s.events.dialogs().pop().expect("a dialog");
    assert_eq!(dialog["proposal"]["scope"], json!("local"));
    assert_eq!(dialog["proposal"]["insecure"], json!(true));
    assert_eq!(
        s.count("SELECT COUNT(*) FROM haex_storage_connections WHERE endpoint_scope = 'local'"),
        2
    );
    assert_eq!(added["bucket"], json!("nas"));
}

#[test]
fn addresses_holzi_never_reaches_are_refused_before_the_dialog() {
    let s = setup();
    s.permit("add", "*", "granted");
    for endpoint in [
        "http://169.254.169.254",
        "https://[fe80::1]",
        "ftp://x.example",
    ] {
        let proposal = add(json!({ "endpoint": endpoint, "region": "eu", "bucket": "b" }));
        assert_eq!(
            s.code("extension_remote_storage_add_backend", proposal),
            3001,
            "{endpoint}"
        );
    }
    assert!(s.events.dialogs().is_empty());
}

/// A name that resolves to a public address first and to a local one after (DNS rebinding).
#[derive(Default)]
struct Rebinding(AtomicUsize);

#[async_trait]
impl Resolver for Rebinding {
    async fn lookup(&self, _host: &str, _port: u16) -> io::Result<Vec<IpAddr>> {
        let first = self.0.fetch_add(1, Ordering::SeqCst) == 0;
        let ip = if first { "93.184.216.34" } else { "10.0.0.5" };
        Ok(vec![ip.parse().expect("ip")])
    }
}

#[test]
fn an_endpoint_that_leaves_its_confirmed_scope_after_the_dialog_is_refused() {
    let s = setup_with(Arc::new(Rebinding::default()));
    s.permit("add", "s3.rebind.example", "granted");
    s.events.answer_with(with_credentials());
    let entries = s.count("SELECT COUNT(*) FROM haex_passwords_item_details");
    let calls = s.fake.calls().len();
    let proposal =
        add(json!({ "endpoint": "https://s3.rebind.example", "region": "eu", "bucket": "b" }));
    let error = s
        .call("extension_remote_storage_add_backend", proposal)
        .unwrap_err();
    assert_eq!(error.code.as_u16(), 3001, "{}", error.message);
    let dialog = s.events.dialogs().pop().expect("a dialog");
    assert_eq!(dialog["proposal"]["scope"], json!("public"));
    assert_eq!(s.fake.calls().len(), calls, "no test reached the provider");
    assert_eq!(s.count("SELECT COUNT(*) FROM haex_storage_connections"), 1);
    assert_eq!(
        s.count("SELECT COUNT(*) FROM haex_passwords_item_details"),
        entries,
        "no credentials kept"
    );
}

#[test]
fn an_ipv6_endpoint_is_asked_for_and_granted_in_brackets() {
    let s = setup();
    let proposal =
        add(json!({ "endpoint": "http://[fd00::1]:9000", "region": "home", "bucket": "b" }));
    let error = s
        .call("extension_remote_storage_add_backend", proposal.clone())
        .unwrap_err();
    assert_eq!(error.code.as_u16(), 1004);
    assert_eq!(
        error.details.expect("details")["target"],
        json!("[fd00::1]:9000")
    );
    s.permit("add", "[fd00::1]:9000", "granted");
    assert_eq!(
        s.code("extension_remote_storage_add_backend", proposal),
        1002,
        "the grant leads to the dialog, which the user cancels"
    );
    let dialog = s.events.dialogs().pop().expect("a dialog");
    assert_eq!(dialog["proposal"]["scope"], json!("local"));
}

#[test]
fn an_aws_proposal_needs_the_add_permission_for_its_regional_host() {
    let s = setup();
    let aws = |region: &str| add(json!({ "region": region, "bucket": "b" }));
    let error = s
        .call("extension_remote_storage_add_backend", aws("eu-central-1"))
        .unwrap_err();
    assert_eq!(error.code.as_u16(), 1004);
    assert_eq!(
        error.details,
        Some(json!({
            "resourceType": "remoteStorage",
            "action": "add",
            "target": "s3.eu-central-1.amazonaws.com"
        }))
    );
    assert_eq!(
        s.code("extension_remote_storage_add_backend", aws("EU/1")),
        3001
    );
    assert!(s.events.dialogs().is_empty(), "refused before any dialog");

    s.permit("add", "s3.eu-central-1.amazonaws.com", "granted");
    assert_eq!(
        s.code("extension_remote_storage_add_backend", aws("us-east-1")),
        1004,
        "the grant names one region"
    );
    assert_eq!(
        s.code("extension_remote_storage_add_backend", aws("eu-central-1")),
        1002,
        "the grant leads to the dialog, which the user cancels"
    );
    let dialog = s.events.dialogs().pop().expect("a dialog");
    assert_eq!(dialog["proposal"]["providerName"], json!("AWS"));
    assert_eq!(dialog["proposal"]["scope"], json!("public"));

    s.permit("add", "*", "granted");
    assert_eq!(
        s.code("extension_remote_storage_add_backend", aws("us-east-1")),
        1002,
        "a grant for every host covers AWS too"
    );
}

#[test]
fn an_existing_connection_can_take_the_new_storage_without_credentials() {
    let s = setup();
    s.permit("add", "*", "granted");
    let connection: String = s
        .vault
        .read_blocking(|q| {
            Ok(
                q.query_row("SELECT id FROM haex_storage_connections", &[], |r| r.get(0))?
                    .expect("connection"),
            )
        })
        .expect("read");
    s.events.answer_with(StorageAnswer::Confirm {
        connection_id: Some(connection.clone()),
        credentials: None,
        name: None,
        bucket: None,
        create_folder: false,
    });
    let proposal = add(json!({ "endpoint": ENDPOINT, "region": REGION, "bucket": "more" }));
    s.call("extension_remote_storage_add_backend", proposal)
        .expect("added");
    let dialog = s.events.dialogs().pop().expect("a dialog");
    assert_eq!(
        dialog["proposal"]["connections"][0]["id"],
        json!(connection)
    );
    assert_eq!(s.count("SELECT COUNT(*) FROM haex_storage_connections"), 1);
    assert_eq!(s.count("SELECT COUNT(*) FROM haex_storages"), 3);
}

#[test]
fn update_test_and_remove_need_read_and_write_and_a_confirmation() {
    let s = setup();
    s.permit("read", &s.notes, "granted");
    for (method, params) in [
        (
            "extension_remote_storage_update_backend",
            json!({ "request": { "backendId": s.notes, "name": "X" } }),
        ),
        (
            "extension_remote_storage_test_backend",
            json!({ "backendId": s.notes }),
        ),
        (
            "extension_remote_storage_remove_backend",
            json!({ "backendId": s.notes }),
        ),
    ] {
        assert_eq!(s.code(method, params), 1004, "{method}");
    }
    s.permit("readWrite", &s.notes, "granted");
    s.events.answer_with(confirm());

    let renamed = s
        .call(
            "extension_remote_storage_update_backend",
            json!({ "request": { "backendId": s.notes, "name": "Umbenannt" } }),
        )
        .expect("update");
    assert_eq!(renamed["name"], json!("Umbenannt"));

    s.events.answer_with(with_credentials());
    let entries_before = s.count("SELECT COUNT(*) FROM haex_passwords_item_details");
    s.call(
        "extension_remote_storage_update_backend",
        json!({ "request": { "backendId": s.notes } }),
    )
    .expect("new credentials from holzi's window");
    assert_eq!(
        s.count("SELECT COUNT(*) FROM haex_passwords_item_details"),
        entries_before,
        "the old entry is replaced, not kept"
    );

    s.events.answer_with(confirm());
    assert_eq!(
        s.call(
            "extension_remote_storage_test_backend",
            json!({ "backendId": s.notes })
        )
        .expect("test"),
        Value::Null
    );
    s.fake.fail(Op::Put, StorageError::Network);
    let failed = s
        .call(
            "extension_remote_storage_test_backend",
            json!({ "backendId": s.notes }),
        )
        .unwrap_err();
    assert_eq!(failed.details, Some(json!({ "kind": "network" })));

    s.call(
        "extension_remote_storage_remove_backend",
        json!({ "backendId": s.notes }),
    )
    .expect("remove");
    assert_eq!(s.count("SELECT COUNT(*) FROM haex_storages"), 1);
    assert!(!s
        .events
        .dialogs()
        .iter()
        .any(|d| d.to_string().contains(ACCESS_KEY)));
}

#[test]
fn closing_the_frame_cancels_the_dialog() {
    let s = setup();
    s.permit("readWrite", &s.notes, "granted");
    *s.events.close_frame.lock().expect("lock") = true;
    assert_eq!(
        s.code(
            "extension_remote_storage_remove_backend",
            json!({ "backendId": s.notes })
        ),
        1002
    );
    assert_eq!(s.count("SELECT COUNT(*) FROM haex_storages"), 2);
}

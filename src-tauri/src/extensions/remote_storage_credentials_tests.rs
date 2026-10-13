//! Credentials typed in holzi's window whose test fails (spec 038 US3, FR-013): the window learns
//! why and stays open for corrected credentials or a cancel; the extension sees only the end, and
//! nothing is kept from a failed try.

use serde_json::{json, Value};

use crate::extensions::remote_storage_dialog::{StorageAnswer, StorageTrial};
use crate::extensions::remote_storage_test_support::{credentials, setup, SECRET};
use crate::remote_storage::test_support::Op;
use crate::remote_storage::{StorageError, TestOutcome};

fn with_credentials() -> StorageAnswer {
    StorageAnswer::Confirm {
        connection_id: None,
        credentials: Some(credentials()),
        name: None,
        bucket: None,
        create_folder: false,
        host_key: None,
    }
}

fn local_proposal() -> Value {
    json!({ "request": { "name": "Neu", "type": "s3", "config": {
        "endpoint": "http://192.168.1.5:9000", "region": "eu", "bucket": "nas-b"
    } } })
}

const DENIED: StorageTrial = StorageTrial::Failed {
    outcome: TestOutcome::AccessDenied,
    leftover_key: None,
    host_key: None,
};

#[test]
fn a_failed_test_asks_again_and_a_cancel_creates_nothing() {
    let s = setup();
    s.permit("add", "*", "granted");
    s.fake.fail(Op::Put, StorageError::AccessDenied);
    s.events
        .answer_in_turn(vec![with_credentials(), StorageAnswer::Cancel]);
    let entries = s.count("SELECT COUNT(*) FROM haex_passwords_item_details");

    let error = s
        .call("extension_remote_storage_add_backend", local_proposal())
        .unwrap_err();

    assert_eq!(error.code.as_u16(), 1002, "{}", error.message);
    assert_eq!(
        error.details, None,
        "the extension never learns the outcome"
    );
    assert_eq!(s.events.trials(), vec![Some(DENIED)]);
    assert_eq!(s.count("SELECT COUNT(*) FROM haex_storage_connections"), 1);
    assert_eq!(
        s.count("SELECT COUNT(*) FROM haex_passwords_item_details"),
        entries,
        "no credentials kept"
    );
}

#[test]
fn corrected_credentials_after_a_failed_test_create_the_storage() {
    let s = setup();
    s.permit("add", "*", "granted");
    s.fake.fail_once(Op::Put, StorageError::AccessDenied);
    s.events
        .answer_in_turn(vec![with_credentials(), with_credentials()]);

    let added = s
        .call("extension_remote_storage_add_backend", local_proposal())
        .expect("added with the second try");

    assert_eq!(added["bucket"], json!("nas-b"));
    assert!(!added.to_string().contains(SECRET));
    assert_eq!(
        s.events.trials(),
        vec![Some(DENIED), Some(StorageTrial::Ended)]
    );
    assert_eq!(s.count("SELECT COUNT(*) FROM haex_storage_connections"), 2);
    assert_eq!(s.events.dialogs().len(), 1, "one dialog for both tries");
}

#[test]
fn new_credentials_of_a_storage_can_be_corrected_after_a_failed_test() {
    let s = setup();
    s.permit("readWrite", &s.notes, "granted");
    s.fake.fail_once(Op::Put, StorageError::AccessDenied);
    s.events
        .answer_in_turn(vec![with_credentials(), with_credentials()]);
    let entries = s.count("SELECT COUNT(*) FROM haex_passwords_item_details");

    s.call(
        "extension_remote_storage_update_backend",
        json!({ "request": { "backendId": s.notes } }),
    )
    .expect("changed with the second try");

    assert_eq!(
        s.events.trials(),
        vec![Some(DENIED), Some(StorageTrial::Ended)]
    );
    assert_eq!(
        s.count("SELECT COUNT(*) FROM haex_passwords_item_details"),
        entries,
        "the old entry is replaced, the failed try kept nothing"
    );
}

#[test]
fn a_leftover_test_object_is_reported_to_holzis_window() {
    let s = setup();
    s.permit("add", "*", "granted");
    s.fake.fail_once(Op::Delete, StorageError::MissingRight);
    s.events
        .answer_in_turn(vec![with_credentials(), StorageAnswer::Cancel]);

    let error = s
        .call("extension_remote_storage_add_backend", local_proposal())
        .unwrap_err();

    assert_eq!(error.code.as_u16(), 1002, "{}", error.message);
    assert!(matches!(
        s.events.trials().as_slice(),
        [Some(StorageTrial::Failed {
            outcome: TestOutcome::MissingRight,
            leftover_key: Some(_),
            ..
        })]
    ));
}

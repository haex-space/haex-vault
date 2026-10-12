//! The connection test against the fake store (research R9, SC-004): all four steps, the reason of
//! a failure, the test object deleted also after a failed step, and its key named when the delete
//! fails too.

use super::probe::{probe, TEST_PREFIX};
use super::test_support::{access, FakeStore, Op};
use super::{StorageError, TestOutcome};

const BUCKET: &str = "holzi-test";

#[tokio::test]
async fn a_working_storage_passes_and_keeps_no_test_object() {
    let store = FakeStore::new();
    let result = probe(&store, &access(BUCKET)).await;
    assert_eq!(result.outcome, TestOutcome::Passed);
    assert_eq!(result.leftover_key, None);
    assert!(store.keys(BUCKET).is_empty(), "the test object is gone");
    let ops: Vec<Op> = store.calls().into_iter().map(|(op, _)| op).collect();
    assert_eq!(ops, [Op::Put, Op::Get, Op::List, Op::Delete]);
    assert!(store
        .calls()
        .iter()
        .all(|(_, key)| key.starts_with(TEST_PREFIX)));
}

#[tokio::test]
async fn a_refused_write_names_its_reason_and_deletes_nothing() {
    let cases = [
        (StorageError::AccessDenied, TestOutcome::AccessDenied),
        (StorageError::NotFound, TestOutcome::BucketMissing),
        (StorageError::MissingRight, TestOutcome::MissingRight),
    ];
    for (error, outcome) in cases {
        let store = FakeStore::new();
        store.fail(Op::Put, error);
        let result = probe(&store, &access(BUCKET)).await;
        assert_eq!(result.outcome, outcome, "{error:?}");
        assert_eq!(result.leftover_key, None);
        assert!(
            !store.calls().iter().any(|(op, _)| *op == Op::Delete),
            "nothing was written, so nothing to delete"
        );
    }
}

#[tokio::test]
async fn an_unreachable_provider_is_named_and_a_possible_object_is_deleted() {
    let store = FakeStore::new();
    store.fail(Op::Put, StorageError::TimedOut);
    let result = probe(&store, &access(BUCKET)).await;
    assert_eq!(result.outcome, TestOutcome::Unreachable);
    assert!(
        store.calls().iter().any(|(op, _)| *op == Op::Delete),
        "a write that ran out of time may have arrived"
    );
}

#[tokio::test]
async fn a_key_that_can_only_write_misses_a_right_and_its_object_is_deleted() {
    let store = FakeStore::new();
    store.fail(Op::Get, StorageError::MissingRight);
    let result = probe(&store, &access(BUCKET)).await;
    assert_eq!(result.outcome, TestOutcome::MissingRight);
    assert_eq!(result.leftover_key, None);
    assert!(
        store.keys(BUCKET).is_empty(),
        "deleted after the failed step"
    );
}

#[tokio::test]
async fn a_failed_delete_names_the_key_of_the_test_object() {
    let store = FakeStore::new();
    store.fail(Op::Delete, StorageError::MissingRight);
    let result = probe(&store, &access(BUCKET)).await;
    assert_eq!(result.outcome, TestOutcome::MissingRight);
    let key = result.leftover_key.expect("the key is named");
    assert!(key.starts_with(TEST_PREFIX));
    assert_eq!(store.keys(BUCKET), [key]);
}

#[tokio::test]
async fn a_failed_listing_after_a_failed_delete_still_names_the_key() {
    let store = FakeStore::new();
    store.fail(Op::List, StorageError::Network);
    store.fail(Op::Delete, StorageError::Network);
    let result = probe(&store, &access(BUCKET)).await;
    assert_eq!(result.outcome, TestOutcome::Unreachable);
    assert!(result.leftover_key.is_some());
}

/// Access to a WebDAV storage on the fake; its folder is there when the fake holds the key "".
fn webdav(folder_there: bool) -> (FakeStore, super::Access) {
    let store = FakeStore::new();
    if folder_there {
        store.insert(BUCKET, "", b"");
    }
    let mut access = access(BUCKET);
    access.location.protocol = super::Protocol::WebDav;
    (store, access)
}

#[tokio::test]
async fn a_webdav_storage_is_asked_for_its_folder_first_and_tidied_after() {
    let (store, access) = webdav(true);
    let result = probe(&store, &access).await;
    assert_eq!(result.outcome, TestOutcome::Passed);
    let calls = store.calls();
    assert_eq!(calls.first(), Some(&(Op::Head, String::new())));
    assert_eq!(
        calls.last(),
        Some(&(Op::Delete, TEST_PREFIX.to_owned())),
        "the empty test folder goes too (spec 050 research R10)"
    );
}

#[tokio::test]
async fn a_missing_folder_is_named_without_writing() {
    let (store, access) = webdav(false);
    let result = probe(&store, &access).await;
    assert_eq!(result.outcome, TestOutcome::FolderMissing);
    assert!(!store.calls().iter().any(|(op, _)| *op == Op::Put));

    let (store, access) = webdav(true);
    store.fail(Op::Head, StorageError::Protocol);
    assert_eq!(probe(&store, &access).await.outcome, TestOutcome::NotWebDav);
}

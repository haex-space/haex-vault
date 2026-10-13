//! Endpoints in the settings (spec 038 research R8): the scope of an endpoint is fixed when it is
//! set and kept until it moves, and an endpoint without addresses fails its test and saves
//! nothing.

// These tests count rows directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use super::model::StorageInput;
use super::service_tests::{count, entries, input, ops, setup, typed};
use super::{EndpointScope, TestOutcome};
use crate::error::HolziError;

#[tokio::test]
async fn the_scope_of_an_endpoint_is_fixed_when_it_is_set() {
    let s = setup();
    let mut public = input(None, Some(typed()));
    public.endpoint = Some("https://s3.example.com".to_owned());
    let view = s.service.save_connection(public).await.expect("public");
    assert_eq!(view.endpoint_scope, EndpointScope::Public);
    assert!(!view.insecure);

    let mut moved = input(Some(view.id.clone()), None);
    moved.endpoint = Some("http://rustfs.lan:9000".to_owned());
    let view = s.service.save_connection(moved).await.expect("local");
    assert_eq!(
        view.endpoint_scope,
        EndpointScope::Local,
        "a changed endpoint gets its scope anew"
    );
    assert!(view.insecure);

    let mut renamed = input(Some(view.id.clone()), None);
    renamed.endpoint = Some("http://rustfs.lan:9000".to_owned());
    renamed.provider_name = "Heim".to_owned();
    let view = s.service.save_connection(renamed).await.expect("renamed");
    assert_eq!(view.endpoint_scope, EndpointScope::Local, "kept");
    let storage = s
        .service
        .save_storage(StorageInput {
            id: None,
            connection_id: view.id.clone(),
            name: "Fotos".to_owned(),
            bucket: "holzi-test".to_owned(),
            create_folder: false,
        })
        .await
        .expect("storage");
    assert_eq!(
        s.service
            .access_of(&storage.id)
            .await
            .expect("access")
            .location
            .endpoint_scope,
        EndpointScope::Local,
        "every call checks against the stored scope"
    );
}

#[tokio::test]
async fn an_endpoint_without_addresses_fails_its_test_and_saves_nothing() {
    let s = setup();
    let mut nowhere = input(None, Some(typed()));
    nowhere.endpoint = Some("https://nowhere.example.com".to_owned());
    assert!(matches!(
        s.service.save_connection(nowhere).await,
        Err(HolziError::StorageTestFailed {
            outcome: TestOutcome::Unreachable,
            leftover_key: None,
            ..
        })
    ));
    assert_eq!(ops(&s.fake), 0);
    assert_eq!(
        count(&s.db, "SELECT COUNT(*) FROM haex_storage_connections"),
        0
    );
    assert_eq!(entries(&s.db), 0);
}

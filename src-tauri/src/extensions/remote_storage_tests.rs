//! Remote storage through the bridge against the fake provider (spec 038 US2, T035, T037): the
//! list by name only, permissions per storage, the extension's own area, limits, the mapping of
//! the provider's errors, and that no answer of any of the nine methods carries a credential, the
//! endpoint or the region (SC-002).

use serde_json::{json, Value};
use uuid::Uuid;

use crate::extensions::bridge::blocking::block_on;
use crate::extensions::error::BridgeError;
use crate::extensions::remote_storage_dialog::StorageAnswer;
use crate::extensions::remote_storage_test_support::{setup, ACCESS_KEY, REGION, SECRET};
use crate::passwords::service::PasswordsService;
use crate::remote_storage::service::StorageService;
use crate::remote_storage::test_support::{resolver, Op};
use crate::remote_storage::StorageError;

fn b64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn object(storage: &str, key: &str) -> Value {
    json!({ "request": { "backendId": storage, "key": key } })
}

#[test]
fn the_list_names_only_the_storages_a_read_permission_covers() {
    let s = setup();
    assert_eq!(
        s.code("extension_remote_storage_list_backends", json!({})),
        1004,
        "without any read permission the extension is asked for one"
    );
    s.permit("read", &s.photos, "granted");
    let listed = s
        .call("extension_remote_storage_list_backends", json!({}))
        .expect("list");
    assert_eq!(
        listed,
        json!([{
            "id": s.photos,
            "type": "s3",
            "name": "Fotos",
            "providerName": "RustFS",
            "bucket": "photos",
        }]),
        "names only (FR-009a)"
    );
}

#[test]
fn objects_go_to_the_extensions_area_and_come_back_without_it() {
    let s = setup();
    s.permit("readWrite", &s.photos, "granted");
    let upload =
        json!({ "request": { "backendId": s.photos, "key": "a/b.txt", "data": b64(b"hello") } });
    s.call("extension_remote_storage_upload", upload)
        .expect("upload");
    assert_eq!(s.fake.keys("photos"), [format!("{}a/b.txt", s.prefix())]);

    let downloaded = s
        .call(
            "extension_remote_storage_download",
            object(&s.photos, "a/b.txt"),
        )
        .expect("download");
    assert_eq!(downloaded, json!(b64(b"hello")));

    let listed = s
        .call(
            "extension_remote_storage_list",
            json!({ "request": { "backendId": s.photos, "prefix": "a/" } }),
        )
        .expect("list");
    assert_eq!(listed[0]["key"], json!("a/b.txt"));
    assert_eq!(listed.as_array().map(Vec::len), Some(1));

    s.call(
        "extension_remote_storage_delete",
        object(&s.photos, "a/b.txt"),
    )
    .expect("delete");
    assert!(s.fake.keys("photos").is_empty());
}

#[test]
fn read_alone_does_not_write_and_an_uncovered_storage_stays_unknown() {
    let s = setup();
    s.permit("read", &s.photos, "granted");
    let puts = || {
        s.fake
            .calls()
            .iter()
            .filter(|(op, _)| *op == Op::Put)
            .count()
    };
    let before = puts();
    let upload = json!({ "request": { "backendId": s.photos, "key": "x", "data": b64(b"x") } });
    assert_eq!(s.code("extension_remote_storage_upload", upload), 1004);
    assert_eq!(
        s.code("extension_remote_storage_download", object(&s.notes, "x")),
        1004,
        "another storage needs its own permission"
    );
    assert_eq!(
        s.code(
            "extension_remote_storage_download",
            object("no-such-storage", "x")
        ),
        1004,
        "without a permission the extension does not learn that a storage is missing"
    );
    s.permit("read", "*", "granted");
    assert_eq!(
        s.code(
            "extension_remote_storage_download",
            object("no-such-storage", "x")
        ),
        1001
    );
    s.permit("read", &s.notes, "denied");
    assert_eq!(
        s.code("extension_remote_storage_download", object(&s.notes, "x")),
        1002
    );
    assert_eq!(puts(), before, "nothing was written");
}

#[test]
fn keys_that_leave_the_area_are_refused_before_the_provider() {
    let s = setup();
    s.permit("readWrite", &s.photos, "granted");
    let before = s.fake.calls().len();
    for key in ["../x", "/abs", "a//b", "a\\b", "a/./b", ""] {
        assert_eq!(
            s.code("extension_remote_storage_download", object(&s.photos, key)),
            3001,
            "{key:?}"
        );
    }
    assert_eq!(
        s.code(
            "extension_remote_storage_list",
            json!({ "request": { "backendId": s.photos, "prefix": "../" } })
        ),
        3001
    );
    assert_eq!(s.fake.calls().len(), before, "nothing reached the provider");
}

#[test]
fn keys_of_the_provider_outside_the_area_are_not_listed() {
    let s = setup();
    s.permit("readWrite", &s.photos, "granted");
    let access = block_on(
        StorageService::new(
            s.vault.clone(),
            PasswordsService::new(s.vault.clone()),
            s.fake.clone(),
            resolver(&[]),
        )
        .access_of(&s.photos),
    )
    .expect("access");
    let soon = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    use crate::remote_storage::RemoteStore;
    block_on(
        s.fake
            .put(&access, "elsewhere/secret.txt", b"x".to_vec(), soon),
    )
    .expect("put");
    let other_area = format!("holzi-ext/{}/{}/x", Uuid::nil(), Uuid::nil());
    block_on(s.fake.put(&access, &other_area, b"x".to_vec(), soon)).expect("put");
    let listed = s
        .call(
            "extension_remote_storage_list",
            json!({ "request": { "backendId": s.photos } }),
        )
        .expect("list");
    assert_eq!(listed, json!([]));
}

#[test]
fn limits_of_the_extension_hold_for_storage_calls() {
    let s = setup();
    s.permit("readWrite", &s.photos, "granted");
    let id = s.ctx.session.extension_id.to_string();
    s.vault
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE extension_limits SET max_response_bytes = 8, max_rows = 1 \
                 WHERE extension_id = ?1",
                &[&id],
            )?;
            Ok(())
        })
        .expect("limits");
    let big = json!({ "request": { "backendId": s.photos, "key": "big", "data": b64(&[1; 12]) } });
    assert_eq!(s.code("extension_remote_storage_upload", big), 7000);
    let small = json!({ "request": { "backendId": s.photos, "key": "a", "data": b64(&[1; 6]) } });
    s.call("extension_remote_storage_upload", small)
        .expect("fits");
    assert_eq!(
        s.code("extension_remote_storage_download", object(&s.photos, "a")),
        0,
        "6 bytes are 8 in base64"
    );
    let second = json!({ "request": { "backendId": s.photos, "key": "b", "data": b64(&[1; 3]) } });
    s.call("extension_remote_storage_upload", second)
        .expect("fits");
    let listed = s.call(
        "extension_remote_storage_list",
        json!({ "request": { "backendId": s.photos } }),
    );
    assert_eq!(
        listed.map_err(|e| e.code.as_u16()),
        Err(7000),
        "more than max_rows"
    );
}

#[test]
fn errors_of_the_provider_reach_the_extension_as_kinds() {
    let s = setup();
    s.permit("readWrite", &s.photos, "granted");
    let kind = |error: BridgeError| (error.code.as_u16(), error.details);
    s.fake.fail(Op::Get, StorageError::NotFound);
    assert_eq!(
        s.code("extension_remote_storage_download", object(&s.photos, "x")),
        1001
    );
    s.fake.fail(Op::Get, StorageError::Network);
    assert_eq!(
        kind(
            s.call("extension_remote_storage_download", object(&s.photos, "x"))
                .unwrap_err()
        ),
        (2002, Some(json!({ "kind": "network" })))
    );
    s.fake.fail(Op::Get, StorageError::AccessDenied);
    assert_eq!(
        kind(
            s.call("extension_remote_storage_download", object(&s.photos, "x"))
                .unwrap_err()
        ),
        (2002, Some(json!({ "kind": "accessDenied" })))
    );
    assert_eq!(
        s.count("SELECT COUNT(*) FROM storage_tests_no_sync WHERE outcome = 'accessDenied'"),
        1,
        "the settings show the hint"
    );
}

#[test]
fn deleted_credentials_are_access_denied_for_the_extension() {
    let s = setup();
    s.permit("readWrite", &s.photos, "granted");
    s.vault
        .write_blocking(|tx| {
            tx.execute("DELETE FROM haex_passwords_item_details", &[])?;
            Ok(())
        })
        .expect("the user deletes the entry");
    let error = s
        .call("extension_remote_storage_download", object(&s.photos, "x"))
        .unwrap_err();
    assert_eq!(
        (error.code.as_u16(), error.details),
        (2002, Some(json!({ "kind": "accessDenied" })))
    );
}

/// SC-002: no answer or error of the nine methods carries a credential, the endpoint or the
/// region, also when the provider fails.
#[test]
fn no_answer_carries_credentials_endpoint_or_region() {
    let s = setup();
    s.permit("readWrite", "*", "granted");
    s.permit("add", "*", "granted");
    s.events.answer_with(StorageAnswer::Confirm {
        connection_id: None,
        credentials: None,
        name: None,
        bucket: None,
        create_folder: false,
        host_key: None,
    });
    let calls = [
        ("extension_remote_storage_list_backends", json!({})),
        (
            "extension_remote_storage_upload",
            json!({ "request": { "backendId": s.photos, "key": "k", "data": b64(b"v") } }),
        ),
        ("extension_remote_storage_download", object(&s.photos, "k")),
        (
            "extension_remote_storage_list",
            json!({ "request": { "backendId": s.photos } }),
        ),
        ("extension_remote_storage_delete", object(&s.photos, "k")),
        (
            "extension_remote_storage_add_backend",
            json!({ "request": { "name": "Neu", "type": "s3", "sameProviderAs": s.photos,
                "config": { "bucket": "fresh" } } }),
        ),
        (
            "extension_remote_storage_update_backend",
            json!({ "request": { "backendId": s.notes, "name": "Umbenannt" } }),
        ),
        (
            "extension_remote_storage_test_backend",
            json!({ "backendId": s.notes }),
        ),
        (
            "extension_remote_storage_remove_backend",
            json!({ "backendId": s.notes }),
        ),
    ];
    let markers = [ACCESS_KEY, SECRET, "127.0.0.1", REGION];
    for failing in [
        None,
        Some(StorageError::AccessDenied),
        Some(StorageError::Network),
    ] {
        if let Some(error) = failing {
            for op in [Op::Put, Op::Get, Op::List, Op::Delete] {
                s.fake.fail(op, error);
            }
        }
        for (method, params) in &calls {
            let answer = match s.call(method, params.clone()) {
                Ok(value) => value.to_string(),
                Err(error) => serde_json::to_string(&error).expect("json"),
            };
            for marker in markers {
                assert!(
                    !answer.contains(marker),
                    "{method} ({failing:?}) leaks {marker}: {answer}"
                );
            }
        }
    }
}

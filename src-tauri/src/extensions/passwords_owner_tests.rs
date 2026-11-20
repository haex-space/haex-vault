//! Entries that belong to a haex-vault function (spec 038, rule Z14) through the bridge: an extension
//! with a `passwords` grant for `*` never reaches one, neither in a list nor by its id (SC-002).

use serde_json::json;

use super::block_on;
use super::tests::setup;
use crate::passwords::access::Caller;
use crate::passwords::model::ItemInput;
use crate::passwords::service::PasswordsService;

const SECRET: &str = "s3-owner-marker-91d2";

#[test]
fn an_extension_with_a_grant_for_everything_never_reaches_an_owned_entry() {
    let s = setup();
    s.permit("readWrite", "*", "granted");
    let owned = block_on(PasswordsService::new(s.vault.clone()).create_owned_item(
        &Caller::Internal { feature: "storage" },
        ItemInput {
            title: Some("S3: RustFS".into()),
            password: Some(SECRET.into()),
            ..ItemInput::default()
        },
    ))
    .expect("create an owned entry");

    let listed = s.call("extension_password_list", json!({})).unwrap();
    assert!(!listed.to_string().contains(&owned), "not in the list");
    assert!(
        listed.to_string().contains(&s.calendar),
        "the user's entries are"
    );
    for (method, params) in [
        ("extension_password_read", json!({ "itemId": owned })),
        (
            "extension_password_update",
            json!({ "itemId": owned, "input": { "title": "x", "tags": [] } }),
        ),
        ("extension_password_delete", json!({ "itemId": owned })),
    ] {
        assert_eq!(
            s.code(method, params),
            1001,
            "{method}: like a missing entry"
        );
    }
    let read = block_on(PasswordsService::new(s.vault.clone()).read_secret_item(
        &Caller::User,
        &[],
        owned,
    ))
    .expect("still there for the user");
    assert_eq!(read.password.as_deref(), Some(SECRET));
}

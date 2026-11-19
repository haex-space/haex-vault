//! The password functions of an extension (spec 017, US10, T104) through the bridge, with the
//! password manager's rules of `specs/034-password-manager/contracts/access.md` and the extension as
//! the caller. The rules themselves are tested in `passwords/access_tests.rs` and
//! `tests/passwords_access.rs`; here: that the bridge hands the right caller and grants to them.

use std::path::Path;
use std::sync::Arc;

use uuid::Uuid;

use super::*;
use crate::extensions::bridge::dispatch::{call as bridge_call, Emit};
use crate::extensions::commands::permissions::{set, PermissionSetArgs};
use crate::extensions::host::ExtensionHost;
use crate::extensions::registry::effective::effective_bundle;
use crate::extensions::registry::install::install;
use crate::passwords::model_references::RefMarkKind;
use crate::passwords::reveal::secret_item;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::{VaultDb, VaultGate};

struct Silent;

impl Emit for Silent {
    fn emit(&self, _event: &str, _payload: Value) {}
}

/// A secret no answer may carry outside a single read.
const MARKER: &str = "s3cr3t-marker-7f1c";

pub(super) struct Setup {
    _dir: tempfile::TempDir,
    pub(super) vault: VaultDb,
    ctx: CallContext,
    /// An entry with the tag `haex-calendar` and one with `private`.
    pub(super) calendar: String,
    pub(super) private: String,
}

fn user_create(vault: &VaultDb, title: &str, tag: &str) -> String {
    let service = PasswordsService::new(vault.clone());
    block_on(service.create_item(
        &Caller::User,
        &[],
        ItemInput {
            title: Some(title.into()),
            username: Some(format!("{title}-user")),
            password: Some(format!("{MARKER}-{title}")),
            otp_secret: Some("JBSWY3DPEHPK3PXP".into()),
            tags: vec![tag.into()],
            key_values: vec![KeyValueInput {
                key: "pin".into(),
                value: Some(format!("{MARKER}-pin")),
            }],
            ..ItemInput::default()
        },
        None,
    ))
    .unwrap()
}

pub(super) fn setup() -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-minimal.xt"),
    )
    .unwrap();
    let extension = install(&vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    let bundle = vault
        .read_blocking(move |q| effective_bundle(q, extension).map_err(Into::into))
        .unwrap()
        .unwrap()
        .bundle_id;
    let host = Arc::new(ExtensionHost::default());
    let calendar = user_create(&vault, "caldav", "haex-calendar");
    let private = user_create(&vault, "bank", "private");
    Setup {
        _dir: dir,
        ctx: CallContext {
            db: vault.clone(),
            session: host.frames.open(extension, bundle, "tab"),
            host,
            device,
            emitter: Arc::new(Silent),
        },
        vault,
        calendar,
        private,
    }
}

impl Setup {
    pub(super) fn call(&self, method: &str, params: Value) -> Result<Value, BridgeError> {
        bridge_call(&self.ctx, method, &params)
    }

    pub(super) fn code(&self, method: &str, params: Value) -> u16 {
        self.call(method, params)
            .map_or_else(|e| e.code.as_u16(), |_| 0)
    }

    pub(super) fn permit(&self, action: &str, target: &str, status: &str) {
        set(
            &self.vault,
            self.ctx.device,
            PermissionSetArgs {
                extension_id: self.ctx.session.extension_id.to_string(),
                kind: "passwords".into(),
                action: action.into(),
                target: target.into(),
                status: status.into(),
                replaces: None,
            },
            2,
        )
        .unwrap();
    }

    pub(super) fn token(&self, id: &str, kind: RefMarkKind, key: Option<&str>) -> String {
        let service = PasswordsService::new(self.vault.clone());
        block_on(service.reference_token(
            &Caller::User,
            id.to_owned(),
            kind,
            key.map(str::to_owned),
        ))
        .unwrap()
    }

    /// The entry as stored, placeholders unresolved.
    fn stored(&self, id: &str) -> SecretItem {
        let id = id.to_owned();
        self.vault
            .read_blocking(move |q| secret_item(q, &id).map_err(Into::into))
            .unwrap()
            .unwrap()
    }

    fn user_title(&self, id: &str) -> Option<String> {
        let service = PasswordsService::new(self.vault.clone());
        block_on(service.read_secret_item(&Caller::User, &[], id.to_owned()))
            .ok()
            .and_then(|item| item.title)
    }
}

fn input(title: &str, tags: &[&str]) -> Value {
    json!({
        "input": {
            "title": title,
            "username": "anna",
            "password": "pw",
            "autofillAliases": { "username": ["login"] },
            "tags": tags,
            "keyValues": [{ "key": "server", "value": "dav.example.org" }],
        }
    })
}

#[test]
fn with_a_tag_grant_the_extension_lists_reads_and_changes_only_entries_of_that_tag() {
    let s = setup();
    s.permit("readWrite", "haex-calendar", "granted");

    let listed = s.call("extension_password_list", json!({})).unwrap();
    let ids: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [s.calendar.as_str()]);
    assert_eq!(listed[0]["tags"], json!(["haex-calendar"]));
    assert!(
        !listed.to_string().contains(MARKER),
        "no secret in a list (Z4)"
    );

    let read = s
        .call("extension_password_read", json!({ "itemId": s.calendar }))
        .unwrap();
    assert_eq!(read["password"], format!("{MARKER}-caldav"));
    assert_eq!(read["otpSecret"], "JBSWY3DPEHPK3PXP");
    assert_eq!(read["keyValues"][0]["key"], "pin");
    assert!(read["keyValues"][0]["id"].is_string());
    assert_eq!(
        s.code("extension_password_read", json!({ "itemId": s.private })),
        1001,
        "outside the scope is not found (Z5)"
    );
    assert_eq!(
        s.code(
            "extension_password_read",
            json!({ "itemId": Uuid::new_v4().to_string() })
        ),
        1001
    );

    let created = s
        .call(
            "extension_password_create",
            input("new", &["haex-calendar"]),
        )
        .unwrap();
    let created = created.as_str().unwrap().to_owned();
    let read = s
        .call("extension_password_read", json!({ "itemId": created }))
        .unwrap();
    assert_eq!(read["autofillAliases"], json!({ "username": ["login"] }));
    assert_eq!(read["keyValues"][0]["value"], "dav.example.org");

    s.call(
        "extension_password_update",
        json!({ "itemId": created, "input": { "title": "renamed", "tags": ["haex-calendar"] } }),
    )
    .unwrap();
    let read = s
        .call("extension_password_read", json!({ "itemId": created }))
        .unwrap();
    assert_eq!(read["title"], "renamed");
    assert_eq!(
        read["password"],
        Value::Null,
        "the SDK sends the whole entry"
    );
    assert_eq!(read["keyValues"], json!([]));
    assert_eq!(
        s.code(
            "extension_password_update",
            json!({ "itemId": s.private, "input": { "title": "x", "tags": ["haex-calendar"] } })
        ),
        1001,
        "an entry outside the scope cannot be pulled into it (Z7)"
    );
    assert_eq!(s.user_title(&s.private).as_deref(), Some("bank"));

    s.call("extension_password_delete", json!({ "itemId": created }))
        .unwrap();
    assert_eq!(
        s.code("extension_password_read", json!({ "itemId": created })),
        1001,
        "deleted means in the trash, which does not exist for the extension (Z8, Z13)"
    );
    assert_eq!(
        s.user_title(&created).as_deref(),
        Some("renamed"),
        "still there for the user"
    );
    assert_eq!(
        s.code("extension_password_delete", json!({ "itemId": s.private })),
        1001
    );
}

#[test]
fn a_write_needs_a_tag_of_the_scope_and_read_alone_writes_nothing() {
    let s = setup();
    s.permit("readWrite", "haex-calendar", "granted");
    for tags in [&[][..], &["private"][..], &["haex-calendar", "private"][..]] {
        assert_eq!(
            s.code("extension_password_create", input("x", tags)),
            1002,
            "{tags:?} (Z6)"
        );
    }
    assert_eq!(
        s.code(
            "extension_password_update",
            json!({ "itemId": s.calendar, "input": { "title": "x", "tags": ["private"] } })
        ),
        1002,
        "an entry may not leave the scope (Z7)"
    );
    assert_eq!(s.user_title(&s.calendar).as_deref(), Some("caldav"));

    let s = setup();
    s.permit("read", "haex-calendar", "granted");
    assert_eq!(
        s.code("extension_password_read", json!({ "itemId": s.calendar })),
        0
    );
    for (method, params) in [
        ("extension_password_create", input("x", &["haex-calendar"])),
        (
            "extension_password_update",
            json!({ "itemId": s.calendar, "input": { "title": "x", "tags": ["haex-calendar"] } }),
        ),
        ("extension_password_delete", json!({ "itemId": s.calendar })),
    ] {
        assert_eq!(s.code(method, params), 1002, "{method} with read");
    }
    assert_eq!(s.user_title(&s.calendar).as_deref(), Some("caldav"));
}

#[test]
fn without_a_grant_haex_asks_only_for_a_permission_in_the_state_ask() {
    let s = setup();
    for method in ["extension_password_list", "extension_password_read"] {
        assert_eq!(
            s.code(method, json!({ "itemId": s.calendar })),
            1002,
            "{method}: nothing declared, nothing to ask (Z3)"
        );
    }

    s.permit("readWrite", "haex-calendar", "ask");
    let asked = s
        .call("extension_password_read", json!({ "itemId": s.calendar }))
        .unwrap_err();
    assert_eq!(asked.code.as_u16(), 1004);
    assert_eq!(
        asked.details,
        Some(json!({"resourceType": "passwords", "action": "read", "target": "haex-calendar"}))
    );
    let asked = s
        .call("extension_password_create", input("x", &["Haex-Calendar"]))
        .unwrap_err();
    assert_eq!(
        asked.details.unwrap()["action"],
        "readWrite",
        "a write asks for read and write"
    );
    assert_eq!(
        s.code("extension_password_create", input("x", &["private"])),
        1002,
        "no question for a tag nobody declared"
    );
}

#[test]
fn malformed_calls_are_refused_before_the_service_is_asked() {
    let s = setup();
    s.permit("readWrite", "*", "granted");
    assert_eq!(s.code("extension_password_read", json!({})), 3001);
    assert_eq!(
        s.code("extension_password_create", json!({ "input": 7 })),
        3001
    );
    assert_eq!(
        s.code(
            "extension_password_create",
            json!({ "input": { "tags": "x" } })
        ),
        3001
    );
}

/// The SDK's `PasswordInput` from a read `PasswordItemFull`, as an extension sends it back.
fn sent_back(read: &Value) -> Value {
    let mut input = read.clone();
    let map = input.as_object_mut().unwrap();
    for field in ["id", "createdAt", "updatedAt"] {
        map.remove(field);
    }
    for kv in map["keyValues"].as_array_mut().unwrap() {
        kv.as_object_mut().unwrap().remove("id");
    }
    map.retain(|_, value| !value.is_null());
    input
}

#[test]
fn sending_back_what_was_read_keeps_placeholders_hidden_fields_and_field_ids() {
    let s = setup();
    s.permit("readWrite", "haex-calendar", "granted");
    // A user name pointing into the scope, a password and a custom field pointing outside it.
    let username = s.token(&s.calendar, RefMarkKind::Username, None);
    let password = s.token(&s.private, RefMarkKind::Password, None);
    let hidden = s.token(&s.private, RefMarkKind::Extra, Some("pin"));
    let target = block_on(PasswordsService::new(s.vault.clone()).create_item(
        &Caller::User,
        &[],
        ItemInput {
            title: Some("shared".into()),
            username: Some(username.clone()),
            password: Some(password.clone()),
            tags: vec!["haex-calendar".into()],
            key_values: vec![
                KeyValueInput {
                    key: "server".into(),
                    value: Some("dav.example.org".into()),
                },
                KeyValueInput {
                    key: "bank-pin".into(),
                    value: Some(hidden.clone()),
                },
            ],
            autofill_aliases: Some(r#"{"username":["login","user"],"password":["pw"]}"#.into()),
            ..ItemInput::default()
        },
        None,
    ))
    .unwrap();
    let read = s
        .call("extension_password_read", json!({ "itemId": target }))
        .unwrap();
    assert_eq!(read["username"], "caldav-user");
    assert_eq!(
        read["password"],
        Value::Null,
        "the source is outside the scope"
    );
    assert_eq!(read["keyValues"].as_array().unwrap().len(), 1);
    let before = s.stored(&target);

    let mut input = sent_back(&read);
    input["title"] = json!("renamed");
    s.call(
        "extension_password_update",
        json!({ "itemId": target, "input": input }),
    )
    .unwrap();
    let after = s.stored(&target);
    assert_eq!(after.title.as_deref(), Some("renamed"));
    assert_eq!(
        after.username.as_deref(),
        Some(username.as_str()),
        "still a placeholder"
    );
    assert_eq!(
        after.password.as_deref(),
        Some(password.as_str()),
        "never seen, kept"
    );
    assert_eq!(after.autofill_aliases, before.autofill_aliases);
    let ids = |item: &SecretItem| {
        item.key_values
            .iter()
            .map(|kv| kv.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ids(&after),
        ids(&before),
        "same rows, the hidden one included"
    );
    assert_eq!(after.key_values[1].value.as_deref(), Some(hidden.as_str()));

    // A changed custom field keeps its row; one the extension drops goes, the hidden one stays.
    let read = s
        .call("extension_password_read", json!({ "itemId": target }))
        .unwrap();
    let mut input = sent_back(&read);
    input["keyValues"] = json!([{ "key": "server", "value": "cal.example.org" }]);
    s.call(
        "extension_password_update",
        json!({ "itemId": target, "input": input }),
    )
    .unwrap();
    let changed = s.stored(&target);
    assert_eq!(ids(&changed), ids(&before));
    assert_eq!(
        changed.key_values[0].value.as_deref(),
        Some("cal.example.org")
    );
    let read = s
        .call("extension_password_read", json!({ "itemId": target }))
        .unwrap();
    let mut input = sent_back(&read);
    input["keyValues"] = json!([]);
    s.call(
        "extension_password_update",
        json!({ "itemId": target, "input": input }),
    )
    .unwrap();
    assert_eq!(ids(&s.stored(&target)), ids(&before)[1..]);
}

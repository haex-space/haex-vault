//! The address book of an extension (spec 051 US5, T072) through the bridge: the extension is the
//! caller, its `contacts` permissions are the grants. The rules themselves are tested in
//! `contacts/access_tests.rs` and the service's tests; here: that the bridge hands the right caller
//! and grants to them and shows an extension nothing beyond them.

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use image::{DynamicImage, ImageFormat, RgbImage};

use super::*;
use crate::contacts::card::format::{self, CardDevice, ContactCard};
use crate::contacts::model::{ContactInput, Head, ValueInput};
use crate::contacts::service::CardInto;
use crate::extensions::bridge::dispatch::{call as bridge_call, Emit};
use crate::extensions::commands::permissions::{set, PermissionSetArgs};
use crate::extensions::host::ExtensionHost;
use crate::extensions::registry::effective::effective_bundle;
use crate::extensions::registry::install::install;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::sync::signing;
use crate::vault_gate::{VaultDb, VaultGate};

struct Silent;

impl Emit for Silent {
    fn emit(&self, _event: &str, _payload: Value) {}
}

struct Setup {
    _dir: tempfile::TempDir,
    vault: VaultDb,
    ctx: CallContext,
    /// Carla, in the group "Arbeit", with a photo.
    colleague: String,
    /// Anna, in the group "Privat", with a Visitenkarte and a value of a kind holzi does not know.
    friend: String,
    /// Bodo, in no group.
    other: String,
}

fn person(given: &str, groups: Vec<String>) -> ContactInput {
    ContactInput {
        head: Head {
            given_name: Some(given.to_owned()),
            organization: Some("Holz GmbH".to_owned()),
            ..Head::default()
        },
        values: vec![ValueInput {
            kind: "email".to_owned(),
            label: "work".to_owned(),
            value: format!("{}@example.org", given.to_lowercase()),
        }],
        groups,
        targets: vec![],
    }
}

/// A small PNG; the service turns it into the stored JPEG.
fn png() -> Vec<u8> {
    let image = RgbImage::from_fn(40, 40, |x, y| image::Rgb([x as u8 * 6, y as u8 * 6, 90]));
    let mut bytes = Vec::new();
    DynamicImage::ImageRgb8(image)
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode png");
    bytes
}

/// The text of a Visitenkarte of another vault.
fn card_text() -> String {
    let secret = [7u8; 32];
    let card = ContactCard {
        version: format::VERSION,
        vault: signing::xonly_public_key(&secret).expect("a key"),
        issued_at: 1_760_000_000,
        list_generation: 1,
        devices: vec![CardDevice {
            device_pubkey: [1; 32],
            endpoint_id: [2; 32],
            iroh_relay: None,
        }],
        nostr_relays: vec!["wss://relay.example.org".to_owned()],
        name: "Anna".to_owned(),
        values: vec![],
    };
    format::sign(card, &secret).expect("sign").0.text()
}

fn setup() -> Setup {
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

    let service = ContactsService::new(vault.clone());
    let user = Caller::User;
    let group = |name: &str| {
        block_on(service.group_save(&user, None, name.into()))
            .unwrap()
            .id
    };
    let (work, private) = (group("Arbeit"), group("Privat"));
    let create = |input| block_on(service.create(&user, &[], input)).unwrap().id;
    let colleague = create(person("Carla", vec![work]));
    let friend = create(person("Anna", vec![private]));
    let other = create(person("Bodo", vec![]));
    block_on(service.set_photo(&user, &colleague, png())).unwrap();
    block_on(service.card_accept(
        &user,
        card_text(),
        CardInto::Contact {
            contact_id: friend.clone(),
        },
    ))
    .unwrap();
    let unknown = friend.clone();
    vault
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO haex_contacts_values (id, contact_id, kind, label, value, position) \
                 VALUES ('v-future', ?1, 'x-future', 'other', 'from a newer holzi', 0)",
                haex_crdt::rusqlite::params![unknown],
            )?;
            Ok(())
        })
        .unwrap();

    let host = Arc::new(ExtensionHost::default());
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
        colleague,
        friend,
        other,
    }
}

impl Setup {
    fn call(&self, method: &str, params: Value) -> Result<Value, BridgeError> {
        bridge_call(&self.ctx, method, &params)
    }

    fn permit(&self, target: &str, status: &str) {
        set(
            &self.vault,
            self.ctx.device,
            PermissionSetArgs {
                extension_id: self.ctx.session.extension_id.to_string(),
                kind: "contacts".into(),
                action: "read".into(),
                target: target.into(),
                status: status.into(),
                replaces: None,
            },
            2,
        )
        .unwrap();
    }

    /// The ids of `extension_contacts_list`, sorted.
    fn listed(&self) -> Vec<String> {
        let answer = self.call("extension_contacts_list", json!({})).unwrap();
        let mut ids: Vec<String> = answer["contacts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|contact| contact["id"].as_str().unwrap().to_owned())
            .collect();
        ids.sort();
        ids
    }

    fn photo_hash(&self) -> String {
        let service = ContactsService::new(self.vault.clone());
        block_on(service.get(&Caller::User, &[], &self.colleague))
            .unwrap()
            .photo
            .unwrap()
    }
}

fn sorted(ids: &[&String]) -> Vec<String> {
    let mut ids: Vec<String> = ids.iter().map(|id| (*id).clone()).collect();
    ids.sort();
    ids
}

fn code(result: Result<Value, BridgeError>) -> u16 {
    result.map_or_else(|e| e.code.as_u16(), |_| 0)
}

#[test]
fn without_a_permission_every_call_is_1002_and_a_missing_contact_looks_the_same() {
    let s = setup();
    for (method, params) in [
        ("extension_contacts_list", json!({})),
        ("extension_contacts_search", json!({ "query": "a" })),
        ("extension_contacts_read", json!({ "id": s.friend })),
        (
            "extension_contacts_read",
            json!({ "id": "no-such-contact" }),
        ),
        (
            "extension_contacts_photo",
            json!({ "hash": s.photo_hash() }),
        ),
    ] {
        assert_eq!(code(s.call(method, params)), 1002, "{method}");
    }
}

#[test]
fn a_permission_in_the_state_ask_asks_for_it() {
    let s = setup();
    s.permit("arbeit", "ask");
    let error = s
        .call("extension_contacts_list", json!({}))
        .expect_err("asks");
    assert_eq!(error.code.as_u16(), 1004);
    assert_eq!(
        error.details,
        Some(json!({ "resourceType": "contacts", "action": "read", "target": "Arbeit" })),
        "the group as the address book spells it"
    );

    s.permit("*", "ask");
    let error = s
        .call("extension_contacts_list", json!({ "group": "Privat" }))
        .expect_err("asks");
    assert_eq!(error.details.unwrap()["target"], "*");
}

#[test]
fn every_contact_with_star_and_only_the_groups_members_with_a_group() {
    let s = setup();
    s.permit("arbeit", "granted");
    assert_eq!(s.listed(), [s.colleague.clone()]);
    assert_eq!(
        code(s.call("extension_contacts_read", json!({ "id": s.friend }))),
        1002
    );
    let found = s
        .call(
            "extension_contacts_search",
            json!({ "query": "example.org" }),
        )
        .unwrap();
    assert_eq!(found["contacts"].as_array().unwrap().len(), 1);

    s.permit("*", "granted");
    assert_eq!(s.listed(), sorted(&[&s.colleague, &s.friend, &s.other]));
    let page = s
        .call(
            "extension_contacts_list",
            json!({ "group": "Arbeit", "offset": 0, "limit": 5 }),
        )
        .unwrap();
    assert_eq!(page["total"], 1);
    assert_eq!(page["contacts"][0]["id"], s.colleague.as_str());
    assert_eq!(page["contacts"][0]["groups"], json!(["Arbeit"]));
    assert_eq!(page["contacts"][0]["emails"], json!(["carla@example.org"]));
}

#[test]
fn a_denied_star_beats_a_granted_group_and_a_denied_group_hides_its_members() {
    let s = setup();
    s.permit("*", "granted");
    s.permit("Privat", "denied");
    assert_eq!(s.listed(), sorted(&[&s.colleague, &s.other]));
    assert_eq!(
        code(s.call("extension_contacts_read", json!({ "id": s.friend }))),
        1002
    );

    s.permit("Arbeit", "granted");
    s.permit("*", "denied");
    assert_eq!(code(s.call("extension_contacts_list", json!({}))), 1002);
    assert_eq!(
        code(s.call("extension_contacts_read", json!({ "id": s.colleague }))),
        1002
    );
}

#[test]
fn the_detail_holds_every_value_and_the_holzi_addresses_but_not_where_the_contact_goes() {
    let s = setup();
    s.permit("*", "granted");
    let detail = s
        .call("extension_contacts_read", json!({ "id": s.friend }))
        .unwrap();
    assert_eq!(detail["displayName"], "Anna");
    assert_eq!(detail["organization"], "Holz GmbH");
    assert_eq!(detail["groups"], json!(["Privat"]));
    let kinds: Vec<&str> = detail["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"email") && kinds.contains(&"x-future"),
        "{kinds:?}"
    );
    let addresses = &detail["holziAddresses"];
    assert_eq!(addresses["devices"].as_array().unwrap().len(), 1);
    assert_eq!(addresses["nostrRelays"], json!(["wss://relay.example.org"]));
    assert!(addresses["npub"].as_str().unwrap().starts_with("npub1"));
    for hidden in ["origin", "targets", "issues", "links", "card"] {
        assert!(
            detail.get(hidden).is_none(),
            "{hidden} must stay inside holzi"
        );
    }
}

#[test]
fn photo_bytes_only_for_a_contact_the_extension_may_read() {
    let s = setup();
    let hash = s.photo_hash();
    s.permit("Privat", "granted");
    assert_eq!(
        code(s.call("extension_contacts_photo", json!({ "hash": hash }))),
        1002
    );
    s.permit("Arbeit", "granted");
    let jpeg = s
        .call("extension_contacts_photo", json!({ "hash": hash }))
        .unwrap();
    let bytes = STANDARD.decode(jpeg.as_str().unwrap()).unwrap();
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "a JPEG");
}

#[test]
fn malformed_parameters_are_a_validation_error() {
    let s = setup();
    s.permit("*", "granted");
    for (method, params) in [
        ("extension_contacts_read", json!({})),
        ("extension_contacts_search", json!({ "query": 3 })),
        ("extension_contacts_list", json!({ "limit": -1 })),
        ("extension_contacts_photo", json!({ "hash": "" })),
    ] {
        assert_eq!(
            s.call(method, params).unwrap_err().code,
            ExtensionErrorCode::Validation,
            "{method}"
        );
    }
}

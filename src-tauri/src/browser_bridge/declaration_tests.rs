//! Tests for the declaration (spec 049, T011): canonical form, the proposed grant, the "never wider"
//! rule and the refusal of declarations without passwords.

use serde_json::{json, Value};

use super::declaration::{canonical, BridgeAccess, DeclarationRequired, Declared};

fn passwords(entries: Value) -> Value {
    json!({ "core": { "passwords": entries } })
}

fn tags(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| t.to_string()).collect()
}

#[test]
fn the_canonical_form_ignores_the_order_of_keys_and_lists() {
    let a = json!({
        "core": {
            "passwords": [{ "target": "web", "operation": "read" }, { "target": "*", "operation": "readWrite" }],
            "database": [{ "target": "x" }]
        }
    });
    let b = json!({
        "core": {
            "database": [{ "target": "x" }],
            "passwords": [{ "operation": "readWrite", "target": "*" }, { "operation": "read", "target": "web" }]
        }
    });
    let ext = json!([{ "name": "core", "extensionPublicKey": "__core__" }]);
    assert_eq!(
        canonical(Some(&a), Some(&ext)),
        canonical(Some(&b), Some(&ext))
    );
    let c = json!({ "core": { "passwords": [{ "target": "*", "operation": "readWrite" }] } });
    assert_ne!(
        canonical(Some(&a), Some(&ext)),
        canonical(Some(&c), Some(&ext))
    );
}

#[test]
fn the_extension_declaration_proposes_read_write_on_everything() {
    let declared = Declared::read(Some(&passwords(
        json!([{ "target": "*", "operation": "readWrite" }]),
    )))
    .expect("declared");
    assert_eq!(declared.proposal(), (BridgeAccess::ReadWrite, None));
    assert!(declared.declares_write());
}

#[test]
fn a_read_declaration_on_tags_proposes_read_on_those_tags() {
    let declared = Declared::read(Some(&passwords(json!([
        { "target": "web" }, { "target": "shop", "operation": "read" }
    ]))))
    .expect("declared");
    assert_eq!(
        declared.proposal(),
        (BridgeAccess::Read, Some(tags(&["shop", "web"])))
    );
    assert!(!declared.declares_write());
}

#[test]
fn missing_or_empty_passwords_are_no_declaration() {
    assert_eq!(Declared::read(None), Err(DeclarationRequired));
    assert_eq!(
        Declared::read(Some(
            &json!({ "core": { "database": [{ "target": "*" }] } })
        )),
        Err(DeclarationRequired)
    );
    assert_eq!(
        Declared::read(Some(&passwords(json!([])))),
        Err(DeclarationRequired)
    );
    assert_eq!(
        Declared::read(Some(&passwords(
            json!([{ "target": "*", "operation": "delete" }])
        ))),
        Err(DeclarationRequired)
    );
}

#[test]
fn other_categories_are_never_granted() {
    let declared = Declared::read(Some(&json!({
        "core": {
            "passwords": [{ "target": "web", "operation": "read" }],
            "database": [{ "target": "*", "operation": "readWrite" }],
            "bookmarks": [{ "target": "*", "operation": "readWrite" }]
        }
    })))
    .expect("declared");
    assert_eq!(
        declared.proposal(),
        (BridgeAccess::Read, Some(tags(&["web"])))
    );
    assert!(!declared.fits(BridgeAccess::ReadWrite, Some(&tags(&["web"]))));
}

#[test]
fn a_grant_may_be_narrower_but_never_wider() {
    let everything = Declared::read(Some(&passwords(
        json!([{ "target": "*", "operation": "readWrite" }]),
    )))
    .expect("declared");
    assert!(everything.fits(BridgeAccess::ReadWrite, None));
    assert!(everything.fits(BridgeAccess::Read, Some(&tags(&["web"]))));

    let web = Declared::read(Some(&passwords(json!([
        { "target": "web", "operation": "readWrite" }, { "target": "*", "operation": "read" }
    ]))))
    .expect("declared");
    assert!(web.fits(BridgeAccess::Read, None));
    assert!(web.fits(BridgeAccess::ReadWrite, Some(&tags(&["web"]))));
    assert!(!web.fits(BridgeAccess::ReadWrite, None));
    assert!(!web.fits(BridgeAccess::ReadWrite, Some(&tags(&["web", "shop"]))));
    assert!(!web.fits(BridgeAccess::ReadWrite, Some(&[])));
}

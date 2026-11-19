//! Tests for `Passphrase` and the argument types that carry it (spec 013, FR-014, FR-015).

use serde_json::json;
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::Passphrase;
use crate::instances::create::CreateInstanceArgs;
use crate::instances::open::OpenInstanceArgs;

/// Distinctive enough that a substring search cannot match by accident.
const SECRET: &str = "correct-horse-battery-staple-7f3a91";

fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}

#[test]
fn debug_output_never_contains_the_passphrase() {
    let passphrase = Passphrase::from(SECRET);
    let open = OpenInstanceArgs {
        name: "vault".into(),
        passphrase: Passphrase::from(SECRET),
    };
    let create = CreateInstanceArgs {
        name: "vault".into(),
        passphrase: Passphrase::from(SECRET),
    };

    let outputs = [
        format!("{passphrase:?}"),
        format!("{passphrase:#?}"),
        format!("{open:?}"),
        format!("{open:#?}"),
        format!("{create:?}"),
        format!("{create:#?}"),
    ];
    for output in outputs {
        assert!(
            !output.contains(SECRET),
            "a Debug output contains the passphrase"
        );
        assert!(
            output.contains("<redacted>"),
            "a Debug output does not show the redaction marker"
        );
    }
}

#[test]
fn passphrase_deserializes_from_a_json_string() {
    let passphrase: Passphrase = serde_json::from_value(json!(SECRET)).expect("a JSON string");
    assert_eq!(passphrase.as_str(), SECRET);
}

#[test]
fn argument_types_keep_the_wire_shape() {
    let open: OpenInstanceArgs =
        serde_json::from_value(json!({ "name": "vault", "passphrase": SECRET })).expect("open");
    assert_eq!(open.name, "vault");
    assert_eq!(open.passphrase.as_str(), SECRET);

    let create: CreateInstanceArgs =
        serde_json::from_value(json!({ "name": "vault", "passphrase": SECRET })).expect("create");
    assert_eq!(create.name, "vault");
    assert_eq!(create.passphrase.as_str(), SECRET);
}

#[test]
fn zeroize_empties_the_passphrase() {
    let mut passphrase = Passphrase::from(SECRET);
    passphrase.zeroize();
    assert_eq!(passphrase.as_str(), "");
}

#[test]
fn passphrase_erases_itself_on_drop() {
    // A compile-time assertion: the type promises to erase its buffer when dropped.
    assert_zeroize_on_drop::<Passphrase>();
}

#[test]
fn passphrase_converts_from_str_and_string() {
    assert_eq!(Passphrase::from(SECRET).as_str(), SECRET);
    assert_eq!(Passphrase::from(SECRET.to_string()).as_str(), SECRET);
}

#[test]
fn a_new_passphrase_needs_at_least_the_minimum_length() {
    let short = Passphrase::from("1234567");
    assert!(matches!(
        short.validate_new(),
        Err(crate::error::HaexError::WeakPassphrase { .. })
    ));
    assert!(Passphrase::from("12345678").validate_new().is_ok());
}

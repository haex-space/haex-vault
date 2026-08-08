use super::RowCapability;
use crate::ucan::predicate::{Predicate, PrimitiveValue};

// ---------------------------------------------------------------------------
// Serde roundtrip per variant
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_row_insert_with_eq_predicate() {
    let cap = RowCapability::RowInsert {
        matches: Predicate::Eq {
            col: "category".into(),
            eq: PrimitiveValue::String("work".into()),
        },
    };
    let json = serde_json::to_string(&cap).unwrap();
    let parsed: RowCapability = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, cap);
}

#[test]
fn roundtrip_row_update_with_and_predicate() {
    let cap = RowCapability::RowUpdate {
        matches: Predicate::And {
            and: vec![
                Predicate::Eq {
                    col: "category".into(),
                    eq: PrimitiveValue::String("work".into()),
                },
                Predicate::Eq {
                    col: "archived".into(),
                    eq: PrimitiveValue::Bool(false),
                },
            ],
        },
    };
    let json = serde_json::to_string(&cap).unwrap();
    let parsed: RowCapability = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, cap);
}

#[test]
fn roundtrip_row_delete_with_starts_with_predicate() {
    let cap = RowCapability::RowDelete {
        matches: Predicate::StartsWith {
            col: "path".into(),
            starts_with: "/tmp/".into(),
        },
    };
    let json = serde_json::to_string(&cap).unwrap();
    let parsed: RowCapability = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, cap);
}

// ---------------------------------------------------------------------------
// Discriminator + serde shape
// ---------------------------------------------------------------------------

#[test]
fn wire_key_is_where_not_matches() {
    let cap = RowCapability::RowInsert {
        matches: Predicate::Eq {
            col: "c".into(),
            eq: PrimitiveValue::Null,
        },
    };
    let json = serde_json::to_value(&cap).unwrap();
    assert_eq!(json["op"], "row_insert");
    assert!(json.get("where").is_some(), "expected `where` key on wire");
    assert!(
        json.get("matches").is_none(),
        "internal field `matches` must not leak on wire"
    );
}

#[test]
fn op_discriminates_between_variants() {
    let insert_json = r#"{"op":"row_insert","where":{"col":"c","eq":"v"}}"#;
    let update_json = r#"{"op":"row_update","where":{"col":"c","eq":"v"}}"#;
    let delete_json = r#"{"op":"row_delete","where":{"col":"c","eq":"v"}}"#;

    let insert: RowCapability = serde_json::from_str(insert_json).unwrap();
    let update: RowCapability = serde_json::from_str(update_json).unwrap();
    let delete: RowCapability = serde_json::from_str(delete_json).unwrap();

    assert!(matches!(insert, RowCapability::RowInsert { .. }));
    assert!(matches!(update, RowCapability::RowUpdate { .. }));
    assert!(matches!(delete, RowCapability::RowDelete { .. }));
}

// ---------------------------------------------------------------------------
// Fail-closed guards
// ---------------------------------------------------------------------------

#[test]
fn rejects_unknown_op_value() {
    // `op` present but not one of the three known variants.
    let json = r#"{"op":"row_read","where":{"col":"c","eq":"v"}}"#;
    let err = serde_json::from_str::<RowCapability>(json).unwrap_err();
    assert!(
        err.to_string().contains("row_read")
            || err.to_string().contains("unknown variant")
            || err.to_string().contains("expected one of"),
        "expected an unknown-variant error, got: {err}"
    );
}

#[test]
fn rejects_missing_op_tag() {
    let json = r#"{"where":{"col":"c","eq":"v"}}"#;
    assert!(serde_json::from_str::<RowCapability>(json).is_err());
}

#[test]
fn rejects_unknown_field_next_to_where() {
    // Same class of fail-open attack that PR #761 fixed on Predicate:
    // an authorisation grammar must reject grammars with extra keys, or
    // an issuer could smuggle in an unmodelled constraint the audience
    // relies on but the puller ignores.
    let json = r#"{"op":"row_insert","where":{"col":"c","eq":"v"},"junk":42}"#;
    let err = serde_json::from_str::<RowCapability>(json).unwrap_err();
    assert!(
        err.to_string().contains("junk") || err.to_string().contains("unknown field"),
        "expected unknown-field error, got: {err}"
    );
}

#[test]
fn rejects_predicate_with_unknown_field() {
    // Inner Predicate must also reject junk; this asserts the parser is
    // recursive-strict, not just top-level.
    let json = r#"{"op":"row_insert","where":{"col":"c","eq":"v","junk":true}}"#;
    assert!(serde_json::from_str::<RowCapability>(json).is_err());
}

// ---------------------------------------------------------------------------
// Accessor
// ---------------------------------------------------------------------------

#[test]
fn matches_accessor_returns_inner_predicate() {
    let pred = Predicate::Eq {
        col: "owner".into(),
        eq: PrimitiveValue::String("alice".into()),
    };
    let cap = RowCapability::RowInsert {
        matches: pred.clone(),
    };
    assert_eq!(cap.matches(), &pred);
}

use super::{Predicate, PrimitiveValue};

// ---------------------------------------------------------------------------
// Leaf-operator parsing
// ---------------------------------------------------------------------------

#[test]
fn parses_eq_with_string() {
    let json = r#"{"col":"category","eq":"work"}"#;
    let p: Predicate = serde_json::from_str(json).unwrap();
    match p {
        Predicate::Eq { col, eq } => {
            assert_eq!(col, "category");
            assert_eq!(eq, PrimitiveValue::String("work".into()));
        }
        other => panic!("expected Eq, got {other:?}"),
    }
}

#[test]
fn parses_eq_with_number() {
    let json = r#"{"col":"priority","eq":3}"#;
    let p: Predicate = serde_json::from_str(json).unwrap();
    match p {
        Predicate::Eq { col, eq } => {
            assert_eq!(col, "priority");
            match eq {
                PrimitiveValue::Number(n) => assert_eq!(n.as_i64(), Some(3)),
                other => panic!("expected Number, got {other:?}"),
            }
        }
        other => panic!("expected Eq, got {other:?}"),
    }
}

#[test]
fn parses_eq_with_bool() {
    let json = r#"{"col":"archived","eq":true}"#;
    let p: Predicate = serde_json::from_str(json).unwrap();
    match p {
        Predicate::Eq { col, eq } => {
            assert_eq!(col, "archived");
            assert_eq!(eq, PrimitiveValue::Bool(true));
        }
        other => panic!("expected Eq, got {other:?}"),
    }
}

#[test]
fn parses_eq_with_null() {
    let json = r#"{"col":"deleted_at","eq":null}"#;
    let p: Predicate = serde_json::from_str(json).unwrap();
    match p {
        Predicate::Eq { col, eq } => {
            assert_eq!(col, "deleted_at");
            assert_eq!(eq, PrimitiveValue::Null);
        }
        other => panic!("expected Eq, got {other:?}"),
    }
}

#[test]
fn parses_in_with_numbers() {
    let json = r#"{"col":"priority","in":[1,2,3]}"#;
    let p: Predicate = serde_json::from_str(json).unwrap();
    match p {
        Predicate::In { col, values } => {
            assert_eq!(col, "priority");
            assert_eq!(values.len(), 3);
        }
        other => panic!("expected In, got {other:?}"),
    }
}

#[test]
fn parses_starts_with() {
    let json = r#"{"col":"table_name","starts_with":"ext_calendar_"}"#;
    let p: Predicate = serde_json::from_str(json).unwrap();
    match p {
        Predicate::StartsWith { col, starts_with } => {
            assert_eq!(col, "table_name");
            assert_eq!(starts_with, "ext_calendar_");
        }
        other => panic!("expected StartsWith, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Boolean combinators
// ---------------------------------------------------------------------------

#[test]
fn parses_and_of_two() {
    let json = r#"{"and":[{"col":"a","eq":1},{"col":"b","in":[2,3]}]}"#;
    let p: Predicate = serde_json::from_str(json).unwrap();
    match p {
        Predicate::And { and } => assert_eq!(and.len(), 2),
        other => panic!("expected And, got {other:?}"),
    }
}

#[test]
fn parses_or_of_two() {
    let json = r#"{"or":[{"col":"a","eq":1},{"col":"a","eq":2}]}"#;
    let p: Predicate = serde_json::from_str(json).unwrap();
    match p {
        Predicate::Or { or } => assert_eq!(or.len(), 2),
        other => panic!("expected Or, got {other:?}"),
    }
}

#[test]
fn parses_not() {
    let json = r#"{"not":{"col":"category","eq":"archived"}}"#;
    let p: Predicate = serde_json::from_str(json).unwrap();
    match p {
        Predicate::Not { not } => match *not {
            Predicate::Eq { col, .. } => assert_eq!(col, "category"),
            other => panic!("expected inner Eq, got {other:?}"),
        },
        other => panic!("expected Not, got {other:?}"),
    }
}

#[test]
fn parses_nested_and_of_or() {
    let json = r#"{"and":[{"or":[{"col":"a","eq":1},{"col":"a","eq":2}]},{"col":"b","eq":3}]}"#;
    let p: Predicate = serde_json::from_str(json).unwrap();
    match p {
        Predicate::And { and } => {
            assert_eq!(and.len(), 2);
            assert!(matches!(and[0], Predicate::Or { .. }));
            assert!(matches!(and[1], Predicate::Eq { .. }));
        }
        other => panic!("expected And, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Rejection of malformed input
// ---------------------------------------------------------------------------

#[test]
fn rejects_unknown_operator() {
    let json = r#"{"col":"a","matches":"regex"}"#;
    let result: Result<Predicate, _> = serde_json::from_str(json);
    assert!(result.is_err(), "unknown operator must be rejected");
}

#[test]
fn rejects_object_in_eq_value() {
    let json = r#"{"col":"meta","eq":{"nested":"obj"}}"#;
    let result: Result<Predicate, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "predicate operand must be primitive, not object"
    );
}

#[test]
fn rejects_array_in_eq_value() {
    let json = r#"{"col":"tags","eq":["a","b"]}"#;
    let result: Result<Predicate, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "predicate operand must be primitive, not array"
    );
}

#[test]
fn rejects_object_in_in_values() {
    let json = r#"{"col":"c","in":[{"nested":"obj"}]}"#;
    let result: Result<Predicate, _> = serde_json::from_str(json);
    assert!(result.is_err(), "in-list entries must be primitive");
}

#[test]
fn rejects_missing_col_in_eq() {
    let json = r#"{"eq":"x"}"#;
    let result: Result<Predicate, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn rejects_empty_object() {
    let json = r#"{}"#;
    let result: Result<Predicate, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Roundtrip
// ---------------------------------------------------------------------------

#[test]
fn eq_serde_roundtrip() {
    let original = Predicate::Eq {
        col: "category".into(),
        eq: PrimitiveValue::String("work".into()),
    };
    let json = serde_json::to_string(&original).unwrap();
    let back: Predicate = serde_json::from_str(&json).unwrap();
    assert_eq!(original, back);
}

#[test]
fn nested_serde_roundtrip() {
    let original = Predicate::And {
        and: vec![
            Predicate::Or {
                or: vec![
                    Predicate::Eq {
                        col: "a".into(),
                        eq: PrimitiveValue::Number(1.into()),
                    },
                    Predicate::Eq {
                        col: "a".into(),
                        eq: PrimitiveValue::Number(2.into()),
                    },
                ],
            },
            Predicate::Not {
                not: Box::new(Predicate::StartsWith {
                    col: "table_name".into(),
                    starts_with: "haex_".into(),
                }),
            },
        ],
    };
    let json = serde_json::to_string(&original).unwrap();
    let back: Predicate = serde_json::from_str(&json).unwrap();
    assert_eq!(original, back);
}

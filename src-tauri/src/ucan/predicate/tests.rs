use super::{Predicate, PrimitiveValue};
use std::collections::HashMap;

fn row_of(pairs: &[(&str, PrimitiveValue)]) -> HashMap<String, PrimitiveValue> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

fn s(v: &str) -> PrimitiveValue {
    PrimitiveValue::String(v.to_string())
}

fn n(v: i64) -> PrimitiveValue {
    PrimitiveValue::Number(v.into())
}

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

// ---------------------------------------------------------------------------
// C.4 — Evaluator
// ---------------------------------------------------------------------------

#[test]
fn eval_eq_matches_string_column() {
    let p: Predicate = serde_json::from_str(r#"{"col":"category","eq":"work"}"#).unwrap();
    let row = row_of(&[("category", s("work")), ("type", s("event"))]);
    assert!(p.eval(&row));
}

#[test]
fn eval_eq_mismatch_returns_false() {
    let p: Predicate = serde_json::from_str(r#"{"col":"category","eq":"work"}"#).unwrap();
    let row = row_of(&[("category", s("home"))]);
    assert!(!p.eval(&row));
}

#[test]
fn eval_eq_type_mismatch_returns_false() {
    // String predicate vs Number column ↔ two different PrimitiveValue
    // variants ↔ never equal, no panic.
    let p: Predicate = serde_json::from_str(r#"{"col":"priority","eq":"3"}"#).unwrap();
    let row = row_of(&[("priority", n(3))]);
    assert!(!p.eval(&row));
}

#[test]
fn eval_in_matches_number() {
    let p: Predicate = serde_json::from_str(r#"{"col":"priority","in":[1,2,3]}"#).unwrap();
    let row = row_of(&[("priority", n(2))]);
    assert!(p.eval(&row));
}

#[test]
fn eval_in_no_match() {
    let p: Predicate = serde_json::from_str(r#"{"col":"priority","in":[1,2,3]}"#).unwrap();
    let row = row_of(&[("priority", n(7))]);
    assert!(!p.eval(&row));
}

#[test]
fn eval_not_starts_with_negates_leaf() {
    let p: Predicate =
        serde_json::from_str(r#"{"not":{"col":"table_name","starts_with":"ext_passwords_"}}"#)
            .unwrap();
    let row = row_of(&[("table_name", s("ext_calendar_v1"))]);
    assert!(p.eval(&row));
}

#[test]
fn eval_starts_with_matches_prefix() {
    let p: Predicate =
        serde_json::from_str(r#"{"col":"table_name","starts_with":"ext_calendar_"}"#).unwrap();
    let row = row_of(&[("table_name", s("ext_calendar_v1"))]);
    assert!(p.eval(&row));
}

#[test]
fn eval_starts_with_type_mismatch_returns_false() {
    // StartsWith requires a string column value. Non-string ↔ false, no panic.
    let p: Predicate = serde_json::from_str(r#"{"col":"count","starts_with":"1"}"#).unwrap();
    let row = row_of(&[("count", n(123))]);
    assert!(!p.eval(&row));
}

#[test]
fn eval_missing_column_is_null_not_error() {
    let p: Predicate = serde_json::from_str(r#"{"col":"missing","eq":"anything"}"#).unwrap();
    let row = row_of(&[]);
    assert!(!p.eval(&row));
}

#[test]
fn eval_missing_column_matches_null_eq() {
    // Symmetric behaviour: a predicate that explicitly matches NULL matches
    // an absent column. Callers who want SQL-style "NULL never equals NULL"
    // must model that in the grammar, not rely on missing-vs-null distinction.
    let p: Predicate = serde_json::from_str(r#"{"col":"missing","eq":null}"#).unwrap();
    let row = row_of(&[]);
    assert!(p.eval(&row));
}

#[test]
fn eval_and_all_match() {
    let p: Predicate = serde_json::from_str(
        r#"{"and":[{"col":"category","eq":"work"},{"col":"priority","in":[1,2,3]}]}"#,
    )
    .unwrap();
    let row = row_of(&[("category", s("work")), ("priority", n(2))]);
    assert!(p.eval(&row));
}

#[test]
fn eval_and_one_fails() {
    let p: Predicate = serde_json::from_str(
        r#"{"and":[{"col":"category","eq":"work"},{"col":"priority","in":[1,2,3]}]}"#,
    )
    .unwrap();
    let row = row_of(&[("category", s("work")), ("priority", n(9))]);
    assert!(!p.eval(&row));
}

#[test]
fn eval_or_second_matches() {
    let p: Predicate = serde_json::from_str(
        r#"{"or":[{"col":"category","eq":"work"},{"col":"category","eq":"personal"}]}"#,
    )
    .unwrap();
    let row = row_of(&[("category", s("personal"))]);
    assert!(p.eval(&row));
}

#[test]
fn eval_or_all_fail() {
    let p: Predicate = serde_json::from_str(
        r#"{"or":[{"col":"category","eq":"work"},{"col":"category","eq":"personal"}]}"#,
    )
    .unwrap();
    let row = row_of(&[("category", s("archived"))]);
    assert!(!p.eval(&row));
}

#[test]
fn eval_not_negates_true() {
    let p: Predicate = serde_json::from_str(r#"{"not":{"col":"a","eq":"x"}}"#).unwrap();
    let row = row_of(&[("a", s("x"))]);
    assert!(!p.eval(&row));
}

#[test]
fn eval_empty_and_is_true() {
    // Vacuous universal — matches SQL and boolean-algebra convention.
    let p = Predicate::And { and: vec![] };
    let row = row_of(&[]);
    assert!(p.eval(&row));
}

#[test]
fn eval_empty_or_is_false() {
    // Vacuous existential — matches SQL and boolean-algebra convention.
    let p = Predicate::Or { or: vec![] };
    let row = row_of(&[]);
    assert!(!p.eval(&row));
}

#[test]
fn eval_deeply_nested() {
    // AND(OR(a=1, a=2), NOT(starts_with(t, "haex_")))
    let p: Predicate = serde_json::from_str(
        r#"{"and":[{"or":[{"col":"a","eq":1},{"col":"a","eq":2}]},{"not":{"col":"t","starts_with":"haex_"}}]}"#,
    )
    .unwrap();
    let row_pass = row_of(&[("a", n(2)), ("t", s("ext_calendar"))]);
    let row_fail_prefix = row_of(&[("a", n(2)), ("t", s("haex_extensions"))]);
    let row_fail_a = row_of(&[("a", n(9)), ("t", s("ext_calendar"))]);
    assert!(p.eval(&row_pass));
    assert!(!p.eval(&row_fail_prefix));
    assert!(!p.eval(&row_fail_a));
}

// ---------------------------------------------------------------------------
// Serde roundtrip
// ---------------------------------------------------------------------------

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

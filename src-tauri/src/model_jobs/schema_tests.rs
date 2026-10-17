//! Tests for the schema subset of a model job's result format (spec 055, `contracts/job-format.md`).

use serde_json::json;

use super::schema::{check_subset, validate, MAX_DEPTH, MAX_PROPERTIES};

fn page_schema() -> serde_json::Value {
    json!({
        "type": "object", "required": ["regions"],
        "properties": { "regions": { "type": "array", "maxItems": 3, "items": {
            "type": "object", "required": ["kind", "box"],
            "properties": {
                "kind": { "enum": ["text", "image", "table"] },
                "box": { "type": "array", "minItems": 4, "maxItems": 4,
                         "items": { "type": "number", "minimum": 0, "maximum": 1000 } },
                "text": { "type": "string", "maxLength": 10 },
                "rows": { "type": "array", "items": { "type": "array", "items": { "type": "string" } } },
                "note": { "type": ["string", "null"] }
            } } } }
    })
}

#[test]
fn the_example_of_the_contract_is_within_the_subset() {
    assert_eq!(check_subset(&page_schema()), Ok(()));
}

#[test]
fn a_keyword_outside_the_subset_is_named_with_its_place() {
    for keyword in ["pattern", "$ref", "oneOf", "anyOf", "allOf", "format"] {
        let schema =
            json!({"type": "object", "properties": {"a": {"type": "string", keyword: "x"}}});
        let error = check_subset(&schema).expect_err(keyword);
        assert!(error.contains(keyword) && error.contains("/a"), "{error}");
    }
}

#[test]
fn a_schema_needs_a_known_type_or_an_enum() {
    assert!(check_subset(&json!({"description": "no type"})).is_err());
    assert!(check_subset(&json!({"type": "date"})).is_err());
    assert!(check_subset(&json!({"type": ["string", "integer"]})).is_err());
    assert!(check_subset(&json!({"type": ["string", "null"]})).is_ok());
    assert!(check_subset(&json!({"enum": ["a", "b"]})).is_ok());
    assert!(check_subset(&json!({"enum": []})).is_err());
    assert!(check_subset(&json!("string")).is_err());
}

#[test]
fn a_required_name_must_be_declared_and_the_bounds_must_be_numbers() {
    assert!(check_subset(&json!({"type": "object", "required": ["a"]})).is_err());
    assert!(check_subset(&json!({"type": "object", "properties": {}, "required": [1]})).is_err());
    assert!(check_subset(&json!({"type": "number", "minimum": "0"})).is_err());
    assert!(check_subset(&json!({"type": "string", "maxLength": -1})).is_err());
    assert!(check_subset(&json!({"type": "object", "additionalProperties": {}})).is_err());
}

#[test]
fn depth_property_count_and_size_are_limited() {
    let mut deep = json!({"type": "string"});
    for _ in 0..MAX_DEPTH {
        deep = json!({"type": "array", "items": deep});
    }
    assert!(check_subset(&deep).unwrap_err().contains("deeper"));

    let many: serde_json::Map<_, _> = (0..=MAX_PROPERTIES)
        .map(|n| (format!("p{n}"), json!({"type": "string"})))
        .collect();
    assert!(check_subset(&json!({"type": "object", "properties": many})).is_err());

    let long = "x".repeat(40 * 1024);
    assert!(check_subset(&json!({"type": "string", "description": long})).is_err());
}

#[test]
fn a_matching_value_passes() {
    let value = json!({"regions": [
        {"kind": "text", "box": [0, 0, 100, 50], "text": "Hallo", "note": null},
        {"kind": "table", "box": [0.5, 1, 2, 3], "rows": [["a", "b"], ["c", ""]]},
    ]});
    assert_eq!(validate(&page_schema(), &value), Ok(()));
}

#[test]
fn a_violation_names_its_path() {
    let cases = [
        (
            json!({"regions": [{"kind": "poem", "box": [0, 0, 1, 1]}]}),
            "/regions/0/kind",
        ),
        (
            json!({"regions": [{"kind": "text", "box": [0, 0, 1]}]}),
            "/regions/0/box",
        ),
        (
            json!({"regions": [{"kind": "text", "box": [0, 0, 1, 2000]}]}),
            "/regions/0/box/3",
        ),
        (
            json!({"regions": [{"kind": "text", "box": [0, 0, 1, 1], "text": "too long text"}]}),
            "/regions/0/text",
        ),
        (
            json!({"regions": [{"kind": "table", "box": [0, 0, 1, 1], "rows": [["a", 1]]}]}),
            "/regions/0/rows/0/1",
        ),
        (json!({"regions": [{"kind": "text"}]}), "`box` is missing"),
        (json!({}), "`regions` is missing"),
        (json!([]), "expected object"),
    ];
    for (value, expected) in cases {
        let error = validate(&page_schema(), &value).expect_err(expected);
        assert!(
            error.contains(expected),
            "{error} should contain {expected}"
        );
    }
}

#[test]
fn more_items_than_allowed_and_an_undeclared_key_are_refused() {
    let four = json!({"regions": [
        {"kind": "text", "box": [0, 0, 1, 1]}, {"kind": "text", "box": [0, 0, 1, 1]},
        {"kind": "text", "box": [0, 0, 1, 1]}, {"kind": "text", "box": [0, 0, 1, 1]},
    ]});
    assert!(validate(&page_schema(), &four)
        .unwrap_err()
        .contains("more items"));

    let extra = json!({"regions": [{"kind": "text", "box": [0, 0, 1, 1], "colour": "red"}]});
    assert!(validate(&page_schema(), &extra)
        .unwrap_err()
        .contains("`colour`"));

    let open = json!({"type": "object", "properties": {"a": {"type": "string"}}, "additionalProperties": true});
    assert_eq!(validate(&open, &json!({"a": "x", "b": 1})), Ok(()));
}

#[test]
fn integers_accept_whole_floats_and_numbers_reject_text() {
    let integer = json!({"type": "integer"});
    assert_eq!(validate(&integer, &json!(3)), Ok(()));
    assert_eq!(validate(&integer, &json!(3.0)), Ok(()));
    assert!(validate(&integer, &json!(3.5)).is_err());
    assert!(validate(&json!({"type": "number"}), &json!("3")).is_err());
    assert!(validate(&json!({"type": "boolean"}), &json!(0)).is_err());
}

#[test]
fn a_string_length_counts_characters_not_bytes() {
    let schema = json!({"type": "string", "minLength": 3, "maxLength": 3});
    assert_eq!(validate(&schema, &json!("äöü")), Ok(()));
    assert!(validate(&schema, &json!("ab")).is_err());
}

#[test]
fn a_null_pair_makes_a_value_optional() {
    let schema = json!({"type": ["integer", "null"]});
    assert_eq!(validate(&schema, &json!(null)), Ok(()));
    assert_eq!(validate(&schema, &json!(2)), Ok(()));
    assert!(validate(&schema, &json!("2")).is_err());
}

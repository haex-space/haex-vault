use serde_json::{json, Value};

use super::as_meant;

fn schema() -> Value {
    json!({ "type": "object", "properties": {
        "read": { "type": "boolean" },
        "limit": { "type": "integer" },
        "score": { "type": "number" },
        "name": { "type": "string" },
        "either": { "type": ["integer", "string"] },
        "untyped": {},
        "filter": { "type": "object", "properties": { "unread": { "type": "boolean" } } }
    } })
}

#[test]
fn yes_and_no_written_as_text_become_booleans() {
    for (sent, meant) in [
        ("True", true),
        ("false", false),
        ("TRUE", true),
        ("False", false),
    ] {
        assert_eq!(
            as_meant(json!({ "read": sent }), &schema()),
            json!({ "read": meant }),
            "{sent}"
        );
    }
}

#[test]
fn numbers_written_as_text_become_numbers() {
    assert_eq!(
        as_meant(json!({ "limit": "5" }), &schema()),
        json!({ "limit": 5 })
    );
    assert_eq!(
        as_meant(json!({ "limit": "-3" }), &schema()),
        json!({ "limit": -3 })
    );
    assert_eq!(
        as_meant(json!({ "score": "2.5" }), &schema()),
        json!({ "score": 2.5 })
    );
    assert_eq!(
        as_meant(json!({ "score": "3" }), &schema()),
        json!({ "score": 3 })
    );
}

#[test]
fn a_value_whose_meaning_is_not_clear_stays_as_sent() {
    for input in [
        json!({ "read": "yes" }),
        json!({ "read": "1" }),
        json!({ "read": " true" }),
        json!({ "limit": "five" }),
        json!({ "limit": "2.5" }),
        json!({ "limit": "+5" }),
        json!({ "limit": "1e3" }),
        json!({ "score": "NaN" }),
        json!({ "score": "inf" }),
        json!({ "score": "" }),
        json!({ "name": "true" }),
        json!({ "either": "5" }),
        json!({ "untyped": "true" }),
        json!({ "filter": { "unread": "true" } }),
        json!({ "unknown": "true" }),
    ] {
        assert_eq!(as_meant(input.clone(), &schema()), input);
    }
}

#[test]
fn values_already_in_the_right_form_and_other_inputs_stay() {
    let right = json!({ "read": true, "limit": 5, "score": 2.5, "name": "x" });
    assert_eq!(as_meant(right.clone(), &schema()), right);
    assert_eq!(as_meant(json!("text"), &schema()), json!("text"));
    assert_eq!(
        as_meant(json!({ "read": "true" }), &json!({})),
        json!({ "read": "true" })
    );
}

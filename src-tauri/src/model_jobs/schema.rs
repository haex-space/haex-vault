//! The schema subset of the result format of a model job (spec 055, `contracts/job-format.md`).
//!
//! The extension hands holzi a schema with every job; holzi never trusts it as code: it is checked
//! against a small subset first ([`check_subset`]) and the model's output is checked against it
//! afterwards ([`validate`]). The subset is a superset of the action subset
//! (`chat/tools/action_tool.rs::schema_in_subset`), which the frontend mirrors and must not grow;
//! this one is only ever read here.
//!
//! Allowed: `type` (one of `object`, `array`, `string`, `integer`, `number`, `boolean`, or a pair
//! with `"null"`), `properties`, `required`, `items`, `enum`, `minimum`, `maximum`, `minLength`,
//! `maxLength`, `minItems`, `maxItems`, `additionalProperties` (boolean) and `description`. Not
//! allowed: `$ref`, `pattern`, `oneOf`/`anyOf`/`allOf`, anything recursive. A schema no value can
//! satisfy (an `enum` value of another type, a minimum above the maximum, a keyword of another
//! type) is refused, so it never costs a model call.

use serde_json::{json, Value};

/// How deep a schema may nest.
pub const MAX_DEPTH: usize = 8;
/// How many properties a schema may declare in all.
pub const MAX_PROPERTIES: usize = 200;
/// Largest schema, as serialized.
pub const MAX_SCHEMA_BYTES: usize = 32 * 1024;

const KEYWORDS: [&str; 13] = [
    "type",
    "properties",
    "required",
    "items",
    "enum",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "minItems",
    "maxItems",
    "additionalProperties",
    "description",
];

/// Checks that a schema stays within the subset and its limits. The error says where and why.
pub fn check_subset(schema: &Value) -> Result<(), String> {
    if schema.to_string().len() > MAX_SCHEMA_BYTES {
        return Err(format!(
            "the schema is larger than {MAX_SCHEMA_BYTES} bytes"
        ));
    }
    let mut properties = 0;
    check_node(schema, "", 1, &mut properties)
}

fn check_node(
    schema: &Value,
    path: &str,
    depth: usize,
    properties: &mut usize,
) -> Result<(), String> {
    let here = if path.is_empty() { "/" } else { path };
    if depth > MAX_DEPTH {
        return Err(format!("{here}: nested deeper than {MAX_DEPTH} levels"));
    }
    let object = schema
        .as_object()
        .ok_or_else(|| format!("{here}: a schema is an object"))?;
    if let Some(keyword) = object.keys().find(|k| !KEYWORDS.contains(&k.as_str())) {
        return Err(format!("{here}: the keyword `{keyword}` is not supported"));
    }
    let has_enum = object.contains_key("enum");
    let types = match object.get("type") {
        Some(declared) => {
            types_of(declared).ok_or_else(|| format!("{here}: unsupported `type`"))?
        }
        None if has_enum => Vec::new(),
        None => return Err(format!("{here}: a schema needs a `type` or an `enum`")),
    };
    // A keyword of another type than the declared one would make the schema unsatisfiable or
    // meaningless; either way no model output could be judged by it.
    for (keywords, wanted) in [
        (
            &["properties", "required", "additionalProperties"][..],
            &["object"][..],
        ),
        (&["items", "minItems", "maxItems"], &["array"]),
        (&["minLength", "maxLength"], &["string"]),
        (&["minimum", "maximum"], &["integer", "number"]),
    ] {
        if let Some(keyword) = keywords.iter().find(|k| object.contains_key(**k)) {
            if !types.iter().any(|t| wanted.contains(t)) {
                return Err(format!(
                    "{here}: `{keyword}` needs the type {}",
                    wanted.join(" or ")
                ));
            }
        }
    }
    if let Some(values) = object.get("enum") {
        match values.as_array() {
            Some(list) if !list.is_empty() => {
                let typed = json!({ "type": object.get("type") });
                if let Some(value) = list
                    .iter()
                    .find(|value| object.contains_key("type") && validate(&typed, value).is_err())
                {
                    return Err(format!(
                        "{here}: the `enum` value {value} is not of the `type`"
                    ));
                }
            }
            _ => return Err(format!("{here}: `enum` is a non-empty list")),
        }
    }
    if let Some(declared) = object.get("properties") {
        let map = declared
            .as_object()
            .ok_or_else(|| format!("{here}: `properties` is an object"))?;
        *properties += map.len();
        if *properties > MAX_PROPERTIES {
            return Err(format!(
                "the schema declares more than {MAX_PROPERTIES} properties"
            ));
        }
        for (name, child) in map {
            check_node(child, &format!("{path}/{name}"), depth + 1, properties)?;
        }
    }
    if let Some(required) = object.get("required") {
        let list = required
            .as_array()
            .filter(|list| list.iter().all(Value::is_string))
            .ok_or_else(|| format!("{here}: `required` is a list of names"))?;
        let declared = object.get("properties").and_then(Value::as_object);
        if let Some(missing) = list
            .iter()
            .filter_map(Value::as_str)
            .find(|name| !declared.is_some_and(|map| map.contains_key(*name)))
        {
            return Err(format!("{here}: `{missing}` is required but not declared"));
        }
    }
    if let Some(items) = object.get("items") {
        check_node(items, &format!("{path}/items"), depth + 1, properties)?;
    }
    for numeric in ["minimum", "maximum"] {
        if object.get(numeric).is_some_and(|v| !v.is_number()) {
            return Err(format!("{here}: `{numeric}` is a number"));
        }
    }
    for count in ["minLength", "maxLength", "minItems", "maxItems"] {
        if object.get(count).is_some_and(|v| v.as_u64().is_none()) {
            return Err(format!("{here}: `{count}` is a whole number of at least 0"));
        }
    }
    if object
        .get("additionalProperties")
        .is_some_and(|v| !v.is_boolean())
    {
        return Err(format!("{here}: `additionalProperties` is true or false"));
    }
    for (low, high) in [
        ("minimum", "maximum"),
        ("minLength", "maxLength"),
        ("minItems", "maxItems"),
    ] {
        let bound = |name| object.get(name).and_then(Value::as_f64);
        if let (Some(low_value), Some(high_value)) = (bound(low), bound(high)) {
            if low_value > high_value {
                return Err(format!("{here}: `{low}` is above `{high}`"));
            }
        }
    }
    Ok(())
}

/// Whether a value of this schema may be of the named type.
pub fn allows_type(schema: &Value, name: &str) -> bool {
    schema
        .get("type")
        .and_then(types_of)
        .is_some_and(|types| types.contains(&name))
}

/// Whether an object of this schema may hold keys it does not declare: with `properties` only
/// when `additionalProperties` is `true`, without them unless it is `false`.
pub fn is_open_object(schema: &Value) -> bool {
    let declared = schema.get("properties").is_some();
    match schema.get("additionalProperties").and_then(Value::as_bool) {
        Some(open) => open,
        None => !declared,
    }
}

/// The types a `type` value names, or `None` when it names something outside the subset. A pair
/// with `"null"` makes the value optional.
fn types_of(declared: &Value) -> Option<Vec<&str>> {
    const KNOWN: [&str; 6] = ["object", "array", "string", "integer", "number", "boolean"];
    match declared {
        Value::String(name) if KNOWN.contains(&name.as_str()) => Some(vec![name.as_str()]),
        Value::Array(pair) if pair.len() == 2 => {
            let names: Vec<&str> = pair.iter().filter_map(Value::as_str).collect();
            let nulls = names.iter().filter(|name| **name == "null").count();
            let known = names.iter().filter(|name| KNOWN.contains(*name)).count();
            (names.len() == 2 && nulls == 1 && known == 1).then_some(names)
        }
        _ => None,
    }
}

/// Checks a value against a schema that [`check_subset`] accepted. The error names the first
/// violation with its path. An object that declares `properties` allows no other keys unless
/// `additionalProperties` is `true`.
pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    validate_at(schema, value, "")
}

fn validate_at(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let here = if path.is_empty() { "/" } else { path };
    if let Some(declared) = schema.get("type") {
        let types = types_of(declared).unwrap_or_default();
        let fits = |name: &str| match name {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "number" => value.is_number(),
            "integer" => {
                value.is_i64()
                    || value.is_u64()
                    || value
                        .as_f64()
                        .is_some_and(|n| n.is_finite() && n.fract() == 0.0)
            }
            "null" => value.is_null(),
            _ => false,
        };
        if !types.iter().any(|name| fits(name)) {
            return Err(format!("{here}: expected {}", types.join(" or ")));
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            return Err(format!("{here}: not one of the allowed values"));
        }
    }
    if let Some(number) = value.as_f64().filter(|_| value.is_number()) {
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64) {
            if number < min {
                return Err(format!("{here}: below the minimum {min}"));
            }
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64) {
            if number > max {
                return Err(format!("{here}: above the maximum {max}"));
            }
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if schema
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|min| length < min)
        {
            return Err(format!("{here}: shorter than allowed"));
        }
        if schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .is_some_and(|max| length > max)
        {
            return Err(format!("{here}: longer than allowed"));
        }
    }
    if let Some(list) = value.as_array() {
        let length = list.len() as u64;
        if schema
            .get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|min| length < min)
        {
            return Err(format!("{here}: fewer items than allowed"));
        }
        if schema
            .get("maxItems")
            .and_then(Value::as_u64)
            .is_some_and(|max| length > max)
        {
            return Err(format!("{here}: more items than allowed"));
        }
        if let Some(items) = schema.get("items") {
            for (index, item) in list.iter().enumerate() {
                validate_at(items, item, &format!("{path}/{index}"))?;
            }
        }
    }
    if let Some(object) = value.as_object() {
        let empty = serde_json::Map::new();
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        if !is_open_object(schema) {
            if let Some(extra) = object.keys().find(|key| !properties.contains_key(*key)) {
                return Err(format!("{here}: the key `{extra}` is not declared"));
            }
        }
        for name in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !object.contains_key(name) {
                return Err(format!("{here}: `{name}` is missing"));
            }
        }
        for (name, child) in properties {
            if let Some(field) = object.get(name) {
                validate_at(child, field, &format!("{path}/{name}"))?;
            }
        }
    }
    Ok(())
}

//! The data rows of a raw contact as holzi's head fields and values, and back (spec 051 SC-004,
//! contracts/android-plugin.md, data-model.md). Only the mimetypes of the contract are read; a row
//! whose value holzi cannot store (an event in a form that is no date, an empty number) is not
//! mapped and stays on the phone untouched, like every row of another mimetype (FR-022).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};
use tauri_plugin_holzi_android::contacts::DataRow;

use super::labels;
use crate::contacts::kinds::{
    self, ADDRESS, ADDRESS_PARTS, EMAIL, EVENT, IM, PHONE, RELATION, SIP, WEBSITE,
};

/// Where the phone holds a head field of holzi: the column of `haex_contacts`, the mimetype and the
/// field of the row.
pub struct HeadField {
    pub column: &'static str,
    pub mimetype: &'static str,
    pub field: &'static str,
}

pub const STRUCTURED_NAME: &str = "StructuredName";

/// Every head field and its place on the phone.
pub const HEAD_FIELDS: [HeadField; 10] = [
    HeadField {
        column: "display_name",
        mimetype: STRUCTURED_NAME,
        field: "display",
    },
    HeadField {
        column: "given_name",
        mimetype: STRUCTURED_NAME,
        field: "given",
    },
    HeadField {
        column: "family_name",
        mimetype: STRUCTURED_NAME,
        field: "family",
    },
    HeadField {
        column: "middle_name",
        mimetype: STRUCTURED_NAME,
        field: "middle",
    },
    HeadField {
        column: "name_prefix",
        mimetype: STRUCTURED_NAME,
        field: "prefix",
    },
    HeadField {
        column: "name_suffix",
        mimetype: STRUCTURED_NAME,
        field: "suffix",
    },
    HeadField {
        column: "nickname",
        mimetype: "Nickname",
        field: "name",
    },
    HeadField {
        column: "organization",
        mimetype: "Organization",
        field: "company",
    },
    HeadField {
        column: "job_title",
        mimetype: "Organization",
        field: "title",
    },
    HeadField {
        column: "note",
        mimetype: "Note",
        field: "note",
    },
];

/// A value on the phone: the data row that holds it and the value as holzi stores it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneValue {
    pub data_id: i64,
    pub kind: String,
    pub label: String,
    pub value: String,
}

/// What holzi knows of one raw contact: its head fields with the rows that hold them, and its
/// values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneSide {
    /// Head column → value; only fields with a value.
    pub head: BTreeMap<String, String>,
    /// Mimetype → data id of the row holding head fields.
    pub head_rows: BTreeMap<String, i64>,
    pub values: Vec<PhoneValue>,
}

/// The kind of a group membership in the comparison (spec 051 T096). It is no kind of
/// `haex_contacts_values`: holzi keeps memberships in `haex_contacts_group_members`, the phone in
/// `GroupMembership` rows; both meet by the group's name, per account on the phone.
pub const GROUP: &str = "group";

/// The id of a membership in the comparison: the group's name, the same on both sides.
pub fn group_value_id(name: &str) -> String {
    format!("group:{}", name.trim())
}

/// The mimetype a value of `kind` is written as; `None` for kinds that never go to a phone.
pub fn mimetype_of(kind: &str) -> Option<&'static str> {
    Some(match kind {
        PHONE => "Phone",
        EMAIL => "Email",
        WEBSITE => "Website",
        SIP => "SipAddress",
        ADDRESS => "StructuredPostal",
        IM => "Im",
        EVENT => "Event",
        RELATION => "Relation",
        GROUP => "GroupMembership",
        _ => return None,
    })
}

fn kind_of(mimetype: &str) -> Option<&'static str> {
    Some(match mimetype {
        "Phone" => PHONE,
        "Email" => EMAIL,
        "Website" => WEBSITE,
        "SipAddress" => SIP,
        "StructuredPostal" => ADDRESS,
        "Im" => IM,
        "Event" => EVENT,
        "Relation" => RELATION,
        "GroupMembership" => GROUP,
        _ => return None,
    })
}

fn text<'a>(fields: &'a Map<String, Json>, key: &str) -> Option<&'a str> {
    fields
        .get(key)
        .and_then(Json::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// The data rows of one raw contact as holzi sees them. The first row of a head mimetype holds the
/// head fields; a further one of the same mimetype is not mapped.
pub fn map_raw(rows: &[DataRow]) -> PhoneSide {
    let mut side = PhoneSide::default();
    for row in rows {
        if let Some(kind) = kind_of(&row.mimetype) {
            if let Some((label, value)) = value_of(kind, &row.fields) {
                side.values.push(PhoneValue {
                    data_id: row.data_id,
                    kind: kind.to_owned(),
                    label,
                    value,
                });
            }
            continue;
        }
        if side.head_rows.contains_key(&row.mimetype)
            || !HEAD_FIELDS
                .iter()
                .any(|field| field.mimetype == row.mimetype)
        {
            continue;
        }
        side.head_rows.insert(row.mimetype.clone(), row.data_id);
        for field in HEAD_FIELDS
            .iter()
            .filter(|field| field.mimetype == row.mimetype)
        {
            if let Some(value) = text(&row.fields, field.field) {
                side.head.insert(field.column.to_owned(), value.to_owned());
            }
        }
    }
    side
}

/// The label and value of a row of `kind`, if holzi can store them.
fn value_of(kind: &str, fields: &Map<String, Json>) -> Option<(String, String)> {
    if kind == GROUP {
        // The plugin names the group; the system's own groups are not mapped (FR-022).
        if fields.get("readOnly").and_then(Json::as_bool) != Some(false) {
            return None;
        }
        return Some((String::new(), text(fields, "title")?.to_owned()));
    }
    let label = labels::from_phone(kind, text(fields, "type"), text(fields, "label"));
    let value = match kind {
        ADDRESS => address_of(fields)?,
        IM => {
            let handle = text(fields, "data")?;
            let service = labels::service(text(fields, "protocol"), text(fields, "customProtocol"))
                .unwrap_or_else(|| "IM".to_owned());
            canonical(
                IM,
                &serde_json::json!({ "service": service, "handle": handle }).to_string(),
            )
        }
        EVENT => {
            let date = date_of(text(fields, "startDate")?)?;
            serde_json::json!({ "date": date }).to_string()
        }
        RELATION => text(fields, "name")?.to_owned(),
        _ => text(fields, "value")?.to_owned(),
    };
    kinds::validate(kind, &label, &value).ok()?;
    Some((label, value))
}

fn address_of(fields: &Map<String, Json>) -> Option<String> {
    let mut parts = Map::new();
    for part in ADDRESS_PARTS {
        if let Some(value) = text(fields, part) {
            parts.insert(part.to_owned(), Json::String(value.to_owned()));
        }
    }
    if parts.is_empty() {
        // An address the phone has only as one text (from a vCard, say) goes into the street.
        parts.insert(
            "street".to_owned(),
            Json::String(text(fields, "formatted")?.to_owned()),
        );
    }
    Some(Json::Object(parts).to_string())
}

/// `YYYY-MM-DD` or `--MM-DD` from the start of an event's date; other forms are not mapped.
fn date_of(raw: &str) -> Option<String> {
    let date = if raw.starts_with("--") {
        raw.get(..7)?
    } else {
        raw.get(..10)?
    };
    let candidate = serde_json::json!({ "date": date }).to_string();
    kinds::validate(EVENT, "birthday", &candidate).ok()?;
    Some(date.to_owned())
}

/// A value in one form for comparing: JSON objects with sorted keys and without empty parts, text
/// trimmed. Two values that only differ in that form are the same value.
pub fn canonical(kind: &str, value: &str) -> String {
    if matches!(kind, ADDRESS | IM | EVENT) {
        if let Ok(Json::Object(map)) = serde_json::from_str::<Json>(value) {
            let sorted: BTreeMap<String, String> = map
                .into_iter()
                .filter_map(|(key, value)| match value {
                    Json::String(text) if !text.trim().is_empty() => {
                        Some((key, text.trim().to_owned()))
                    }
                    _ => None,
                })
                .collect();
            return serde_json::to_string(&sorted).unwrap_or_default();
        }
    }
    value.trim().to_owned()
}

/// Whether two values of a kind are the same. Phone numbers are the same in other notations:
/// equal digits, or one ends with the other of at least seven digits (`+49 170 1234567` and
/// `0170 1234567`, research R8).
pub fn same_value(kind: &str, a: &str, b: &str) -> bool {
    if kind == PHONE {
        let (a, b) = (super::merge::phone_digits(a), super::merge::phone_digits(b));
        return a == b || super::merge::phones_match(&a, &b);
    }
    if kind == EMAIL {
        return a.trim().eq_ignore_ascii_case(b.trim());
    }
    canonical(kind, a) == canonical(kind, b)
}

/// The fields of a new row for a value; `None` for kinds that never go to a phone.
pub fn insert_fields(
    kind: &str,
    label: &str,
    value: &str,
) -> Option<(&'static str, Map<String, Json>)> {
    let mimetype = mimetype_of(kind)?;
    let mut fields = value_fields(kind, value)?;
    fields.extend(label_fields(kind, label));
    Some((mimetype, fields))
}

/// The fields that change a row from `old` to the new label and value; only what changed.
pub fn update_fields(old: &PhoneValue, label: &str, value: &str) -> Map<String, Json> {
    let mut fields = Map::new();
    if canonical(&old.kind, &old.value) != canonical(&old.kind, value) {
        fields.extend(value_fields(&old.kind, value).unwrap_or_default());
    }
    if old.label != label {
        fields.extend(label_fields(&old.kind, label));
    }
    fields
}

fn label_fields(kind: &str, label: &str) -> Map<String, Json> {
    let mut fields = Map::new();
    if kind == GROUP {
        return fields;
    }
    let (code, own) = labels::to_phone(kind, label);
    fields.insert("type".to_owned(), Json::String(code.to_string()));
    fields.insert("label".to_owned(), own.map_or(Json::Null, Json::String));
    fields
}

fn value_fields(kind: &str, value: &str) -> Option<Map<String, Json>> {
    let mut fields = Map::new();
    match kind {
        ADDRESS => {
            let parts = parsed(value);
            for part in ADDRESS_PARTS {
                let text = parts.get(part).and_then(Json::as_str).map(str::to_owned);
                fields.insert(part.to_owned(), text.map_or(Json::Null, Json::String));
            }
            // The phone builds the one-line form from the parts again.
            fields.insert("formatted".to_owned(), Json::Null);
        }
        IM => {
            let parts = parsed(value);
            let handle = parts.get("handle").and_then(Json::as_str)?.to_owned();
            let service = parts.get("service").and_then(Json::as_str)?.to_owned();
            fields.insert("data".to_owned(), Json::String(handle));
            fields.insert(
                "protocol".to_owned(),
                Json::String(labels::CUSTOM_PROTOCOL.to_string()),
            );
            fields.insert("customProtocol".to_owned(), Json::String(service));
        }
        EVENT => {
            let date = parsed(value).get("date").and_then(Json::as_str)?.to_owned();
            fields.insert("startDate".to_owned(), Json::String(date));
        }
        RELATION => {
            fields.insert("name".to_owned(), Json::String(value.to_owned()));
        }
        PHONE | EMAIL | WEBSITE | SIP => {
            fields.insert("value".to_owned(), Json::String(value.to_owned()));
        }
        // The title until the run puts the group's id of the raw contact's account in its place.
        GROUP => {
            fields.insert("title".to_owned(), Json::String(value.trim().to_owned()));
        }
        _ => return None,
    }
    Some(fields)
}

fn parsed(value: &str) -> Map<String, Json> {
    match serde_json::from_str::<Json>(value) {
        Ok(Json::Object(map)) => map,
        _ => Map::new(),
    }
}

/// The fields of a head row for the given head values (column → value); `None` clears the field.
/// Columns of other mimetypes are left out.
pub fn head_fields(mimetype: &str, values: &BTreeMap<&str, Option<&str>>) -> Map<String, Json> {
    let mut fields = Map::new();
    for field in HEAD_FIELDS
        .iter()
        .filter(|field| field.mimetype == mimetype)
    {
        if let Some(value) = values.get(field.column) {
            let json = value.map_or(Json::Null, |text| Json::String(text.to_owned()));
            fields.insert(field.field.to_owned(), json);
        }
    }
    fields
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
mod tests;

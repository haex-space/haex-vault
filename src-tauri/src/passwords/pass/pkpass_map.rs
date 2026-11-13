//! The mapping of `pass.json` to a pass (spec 092, FR-028, FR-020): the style gives the kind, the
//! organisation the provider, the relevant date the start (its offset is the zone of the place),
//! the expiration date the end, the barcodes the codes, the fields and back fields the free fields,
//! the logo the icon. Anything not kept is named for the report; nothing is lost silently. Nothing
//! here reads a network address: `webServiceURL` is only named as not used (FR-029).

use std::collections::{BTreeSet, HashMap};

use serde_json::{Map, Value};

use super::codes::{check, CodeFormat};
use super::model::{CodeInput, PassKind, Start};
use super::pkpass::{Entries, NotKept, ParsedPass, Refusal, WHY_NOT_SUPPORTED, WHY_UPDATE_SERVICE};
use super::pkpass_strings;
use super::store::{is_real_date, FLAG_INVALID_VALUE, FLAG_UNKNOWN_FORMAT};
use crate::passwords::clock;
use crate::passwords::model::KeyValueInput;

/// Why something is not kept: the file gave it in a form that cannot be read.
const WHY_UNREADABLE: &str = "unreadable";
/// The most codes kept from one pass.
const MAX_CODES: usize = 20;
/// The most free fields kept from one pass.
const MAX_FIELDS: usize = 100;

/// The five styles of a pass and what each becomes: `(key, kind, ticket-like)`.
/// One style of a pass: its key in the file, the kind it gives and whether it is ticket-like.
type Style = (&'static str, fn() -> PassKind, bool);
const STYLES: [Style; 5] = [
    ("boardingPass", || PassKind::BoardingPass, true),
    ("eventTicket", || PassKind::Ticket, true),
    ("coupon", || PassKind::Voucher, false),
    ("storeCard", || PassKind::LoyaltyCard, false),
    ("generic", || PassKind::Other, false),
];
const FIELD_LISTS: [&str; 5] = [
    "headerFields",
    "primaryFields",
    "secondaryFields",
    "auxiliaryFields",
    "backFields",
];
/// Keys of the file that are used, or are only the identity of the file; they are not reported.
const USED_KEYS: [&str; 13] = [
    "formatVersion",
    "passTypeIdentifier",
    "serialNumber",
    "teamIdentifier",
    "organizationName",
    "description",
    "logoText",
    "relevantDate",
    "relevantDates",
    "expirationDate",
    "barcode",
    "barcodes",
    "voided",
];
/// The attributes of a field that are used.
const USED_FIELD_KEYS: [&str; 3] = ["key", "label", "value"];

pub(super) fn map(entries: Entries, language: &str) -> Result<ParsedPass, Refusal> {
    let root: Value =
        serde_json::from_slice(&entries.pass_json).map_err(|_| Refusal::BadPassJson)?;
    let root = root.as_object().ok_or(Refusal::BadPassJson)?;
    if root
        .get("formatVersion")
        .is_some_and(|v| v.as_i64() != Some(1))
    {
        return Err(Refusal::UnsupportedVersion);
    }
    let (style_key, kind, ticket_like) = STYLES
        .iter()
        .find(|(key, _, _)| root.get(*key).is_some_and(Value::is_object))
        .map(|(key, kind, ticket)| (*key, kind(), *ticket))
        .ok_or(Refusal::NoStyle)?;
    let style = root[style_key].as_object().ok_or(Refusal::NoStyle)?;
    let strings = strings_of(&entries.strings, language);
    let tr = |text: &str| {
        strings
            .get(text)
            .cloned()
            .unwrap_or_else(|| text.to_owned())
    };
    let text_of = |key: &str| {
        root.get(key)
            .and_then(Value::as_str)
            .map(|t| tr(t).trim().to_owned())
            .filter(|t| !t.is_empty())
    };

    let mut not_kept = Vec::new();
    let provider = text_of("organizationName");
    let start = start_of(root, &mut not_kept);
    let expires_at = root
        .get("expirationDate")
        .and_then(Value::as_str)
        .and_then(|text| {
            let day = text.trim().get(..10)?;
            is_real_date(day).then(|| day.to_owned())
        });
    if root.get("expirationDate").is_some() && expires_at.is_none() {
        not_kept.push(note("expirationDate", WHY_UNREADABLE));
    }
    let (key_values, attributes) = fields_of(style, &tr);
    let title = title_of(
        style_key,
        style,
        &tr,
        text_of("logoText"),
        text_of("description"),
        provider.as_deref(),
        start.as_ref(),
        language,
    );
    let codes = codes_of(root);
    report(
        root,
        style,
        &attributes,
        &entries.other_images,
        &mut not_kept,
    );
    if root.get("voided").and_then(Value::as_bool) == Some(true) {
        not_kept.push(note("voided", WHY_NOT_SUPPORTED));
    }
    let source_id = match (
        root.get("passTypeIdentifier").and_then(Value::as_str),
        root.get("serialNumber").and_then(Value::as_str),
    ) {
        (Some(kind), Some(serial)) if !kind.is_empty() && !serial.is_empty() => {
            Some(format!("{kind}/{serial}"))
        }
        _ => None,
    };
    Ok(ParsedPass {
        pass_kind: kind,
        ticket_like,
        title,
        provider,
        start,
        expires_at,
        codes,
        key_values,
        icon: entries.logo,
        source_id,
        not_kept,
        signed: entries.signed,
    })
}

fn note(what: &str, why: &str) -> NotKept {
    NotKept {
        what: what.to_owned(),
        why: why.to_owned(),
    }
}

/// The strings of the language of the window: `de` for `de`, then the first `de-…` or `de_…`.
fn strings_of(all: &HashMap<String, Vec<u8>>, language: &str) -> HashMap<String, String> {
    let language = language.to_ascii_lowercase();
    let base = language.split(['-', '_']).next().unwrap_or_default();
    let exact = all.get(&language);
    let by_base = all.get(base);
    let by_region = {
        let mut names: Vec<&String> = all
            .keys()
            .filter(|name| name.split(['-', '_']).next() == Some(base))
            .collect();
        names.sort();
        names.first().and_then(|name| all.get(*name))
    };
    exact
        .or(by_base)
        .or(by_region)
        .map(|bytes| pkpass_strings::parse(bytes))
        .unwrap_or_default()
}

/// The start from `relevantDate`, or the first start of `relevantDates`. A date alone is a date;
/// a moment carries the offset the file gives as the zone of the place (FR-028).
fn start_of(root: &Map<String, Value>, not_kept: &mut Vec<NotKept>) -> Option<Start> {
    let text = root
        .get("relevantDate")
        .and_then(Value::as_str)
        .or_else(|| {
            root.get("relevantDates")?
                .as_array()?
                .iter()
                .find_map(|d| d.get("startDate")?.as_str())
        })?
        .trim();
    let parsed = if text.len() == 10 {
        is_real_date(text).then(|| Start::Date {
            date: text.to_owned(),
        })
    } else {
        text.get(..10)
            .filter(|day| is_real_date(day))
            .and_then(|_| clock::parse_iso_millis(text).zip(offset_of(text)))
            .map(|(ms, zone)| Start::At {
                at: clock::format_millis(ms),
                zone,
            })
    };
    if parsed.is_none() {
        not_kept.push(note("relevantDate", WHY_UNREADABLE));
    }
    parsed
}

/// The offset a time names, as `+HH:MM`; `+00:00` when it names none or `Z`; `None` for an offset
/// that no place has (hours past 23, minutes past 59).
fn offset_of(text: &str) -> Option<String> {
    let rest = text.split_once(['T', ' ']).map_or("", |(_, rest)| rest);
    let Some(at) = rest.find(['Z', 'z', '+', '-']) else {
        return Some("+00:00".to_owned());
    };
    let zone = &rest[at..];
    if zone.starts_with(['Z', 'z']) {
        return Some("+00:00".to_owned());
    }
    let digits: String = zone[1..].chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 4
        || digits[..2].parse::<u8>().ok()? > 23
        || digits[2..].parse::<u8>().ok()? > 59
    {
        return None;
    }
    Some(format!("{}{}:{}", &zone[..1], &digits[..2], &digits[2..]))
}

/// The free fields of a pass, in the order of the file, and the attributes of fields that are not
/// kept. A name that is already taken gets a number, so no field overwrites another (FR-028).
fn fields_of(
    style: &Map<String, Value>,
    tr: &dyn Fn(&str) -> String,
) -> (Vec<KeyValueInput>, BTreeSet<String>) {
    let mut out: Vec<KeyValueInput> = Vec::new();
    let mut attributes = BTreeSet::new();
    for list in FIELD_LISTS {
        let Some(fields) = style.get(list).and_then(Value::as_array) else {
            continue;
        };
        for field in fields.iter().filter_map(Value::as_object) {
            for key in field
                .keys()
                .filter(|k| !USED_FIELD_KEYS.contains(&k.as_str()))
            {
                attributes.insert(key.clone());
            }
            let value = match field.get("value") {
                Some(Value::String(text)) => tr(text),
                Some(Value::Number(number)) => number.to_string(),
                Some(Value::Bool(flag)) => flag.to_string(),
                _ => continue,
            };
            let value = value.trim().to_owned();
            let label = field
                .get("label")
                .and_then(Value::as_str)
                .map(|l| tr(l).trim().to_owned())
                .filter(|l| !l.is_empty())
                .or_else(|| {
                    field
                        .get("key")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .filter(|k| !k.is_empty())
                })
                .unwrap_or_default();
            if value.is_empty() || out.len() >= MAX_FIELDS {
                continue;
            }
            let name = unique_name(&label, &out);
            out.push(KeyValueInput {
                key: name,
                value: Some(value),
            });
        }
    }
    (out, attributes)
}

fn unique_name(label: &str, taken: &[KeyValueInput]) -> String {
    let base = if label.is_empty() { "-" } else { label };
    let exists = |name: &str| taken.iter().any(|kv| kv.key.eq_ignore_ascii_case(name));
    if !exists(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base} ({n})"))
        .find(|name| !exists(name))
        .unwrap_or_else(|| base.to_owned())
}

/// A speaking title (FR-020): for a boarding pass the route with the start, for an event the
/// name of the event, otherwise the logo text or the description; the provider's name is always in
/// it, so the entry is found by the search that exists.
#[allow(clippy::too_many_arguments)]
fn title_of(
    style_key: &str,
    style: &Map<String, Value>,
    tr: &dyn Fn(&str) -> String,
    logo_text: Option<String>,
    description: Option<String>,
    provider: Option<&str>,
    start: Option<&Start>,
    language: &str,
) -> String {
    let primary: Vec<String> = style
        .get("primaryFields")
        .and_then(Value::as_array)
        .map(|fields| {
            fields
                .iter()
                .filter_map(|f| match f.get("value")? {
                    Value::String(text) => Some(tr(text).trim().to_owned()),
                    _ => None,
                })
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let what = match style_key {
        "boardingPass" if primary.len() >= 2 => Some(format!("{} → {}", primary[0], primary[1])),
        "boardingPass" | "eventTicket" => primary.first().cloned(),
        _ => None,
    };
    let mut parts: Vec<String> = Vec::new();
    for part in [logo_text, what.clone()].into_iter().flatten() {
        if !parts.iter().any(|p| p.eq_ignore_ascii_case(&part)) {
            parts.push(part);
        }
    }
    if what.is_none() && parts.is_empty() {
        parts.extend(description);
    }
    let mut title = parts.join(" ");
    if let Some(day) = start.and_then(|s| short_day(s, language)) {
        if !title.is_empty() {
            title = format!("{title}, {day}");
        }
    }
    match provider {
        Some(org) if title.is_empty() => org.to_owned(),
        Some(org) if !title.to_lowercase().contains(&org.to_lowercase()) => {
            format!("{org}: {title}")
        }
        _ if title.is_empty() => "Pass".to_owned(),
        _ => title,
    }
}

/// The day of a start, as the language writes it (`14.11.` in German, `2026-11-14` otherwise).
fn short_day(start: &Start, language: &str) -> Option<String> {
    let day = match start {
        Start::Date { date } => date.as_str(),
        // The day at the place: the moment is UTC, the zone is only the offset the file gave, so
        // the date part of the moment as the file wrote it is what the traveller sees.
        Start::At { at, zone } => {
            let ms = clock::parse_iso_millis(at)?;
            let shifted = clock::format_millis(ms + offset_millis(zone));
            return Some(format_day(shifted.get(..10)?, language));
        }
    };
    Some(format_day(day, language))
}

fn offset_millis(zone: &str) -> i64 {
    let digits: String = zone.chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 4 {
        return 0;
    }
    let minutes =
        digits[..2].parse::<i64>().unwrap_or(0) * 60 + digits[2..].parse::<i64>().unwrap_or(0);
    let sign = if zone.starts_with('-') { -1 } else { 1 };
    sign * minutes * 60_000
}

fn format_day(iso: &str, language: &str) -> String {
    if language.to_ascii_lowercase().starts_with("de") {
        if let (Some(month), Some(day)) = (iso.get(5..7), iso.get(8..10)) {
            return format!("{day}.{month}.");
        }
    }
    iso.to_owned()
}

/// The codes of a pass: `barcodes`, or the legacy single `barcode`. A code whose format holzi does
/// not draw is kept as text with the flag `unknown_format`, one whose value does not fit its format
/// with `invalid_value` (FR-003).
fn codes_of(root: &Map<String, Value>) -> Vec<CodeInput> {
    let listed: Vec<&Value> = match root.get("barcodes").and_then(Value::as_array) {
        Some(list) if !list.is_empty() => list.iter().collect(),
        _ => root.get("barcode").into_iter().collect(),
    };
    let mut codes = Vec::new();
    for entry in listed.into_iter().filter_map(Value::as_object) {
        let Some(message) = entry
            .get("message")
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty())
        else {
            continue;
        };
        let raw = entry.get("format").and_then(Value::as_str).unwrap_or("");
        let name = raw
            .strip_prefix("PKBarcodeFormat")
            .unwrap_or(raw)
            .to_lowercase();
        let known = CodeFormat::parse(&name);
        let flag = match known {
            None => Some(FLAG_UNKNOWN_FORMAT.to_owned()),
            Some(format) => check(format, message)
                .err()
                .map(|_| FLAG_INVALID_VALUE.to_owned()),
        };
        codes.push(CodeInput {
            id: None,
            format: if name.is_empty() {
                "unknown".to_owned()
            } else {
                name
            },
            value: message.to_owned(),
            label: entry
                .get("altText")
                .and_then(Value::as_str)
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty()),
            position: codes.len() as i64,
            flag,
        });
        if codes.len() >= MAX_CODES {
            break;
        }
    }
    codes
}

/// Names what the file holds and holzi does not keep.
fn report(
    root: &Map<String, Value>,
    style: &Map<String, Value>,
    attributes: &BTreeSet<String>,
    images: &[String],
    not_kept: &mut Vec<NotKept>,
) {
    for key in root.keys() {
        if USED_KEYS.contains(&key.as_str())
            || STYLES.iter().any(|(style_key, _, _)| style_key == key)
        {
            continue;
        }
        let why = if key == "webServiceURL" || key == "authenticationToken" {
            WHY_UPDATE_SERVICE
        } else {
            WHY_NOT_SUPPORTED
        };
        not_kept.push(note(key, why));
    }
    for key in style.keys().filter(|k| !FIELD_LISTS.contains(&k.as_str())) {
        not_kept.push(note(key, WHY_NOT_SUPPORTED));
    }
    for attribute in attributes {
        not_kept.push(note(&format!("fields.{attribute}"), WHY_NOT_SUPPORTED));
    }
    for image in images {
        not_kept.push(note(image, WHY_NOT_SUPPORTED));
    }
}

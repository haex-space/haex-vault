//! The password methods of the bridge (spec 049, US2, US3, FR-019 to FR-027, research R7 to R9,
//! `contracts/wire-protocol.md`): finding entries for an address, the TOTP code, saving a new entry
//! and the generator presets. Every answer is built from chosen fields; no model of the service is
//! passed through (FR-040).

use serde::Deserialize;
use serde_json::{json, Map, Value};
use url::Url;

use super::{service_of, to_code, BoxedOutcome, Call, Outcome};
use crate::browser_bridge::protocol::ErrorCode;
use crate::passwords::model::{ItemInput, Preset, SecretItem};
use crate::passwords::service::Headers;
use crate::passwords::site::site_of;

/// For the contract test: where these handlers live (FR-041).
pub const MODULE: &str = module_path!();

/// The title of an entry without one, as haex-vault 2.x (Legacy) answers.
pub(super) const UNTITLED: &str = "Untitled";

#[derive(Deserialize)]
struct GetItems {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    fields: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetTotp {
    #[serde(default)]
    entry_id: Option<String>,
}

/// Exactly the four fields `create-item` takes (FR-022); serde passes over every other.
#[derive(Deserialize)]
struct CreateItem {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

pub(super) fn input<T: for<'de> Deserialize<'de>>(payload: Value) -> Result<T, ErrorCode> {
    serde_json::from_value(payload).map_err(|_| ErrorCode::InvalidRequest)
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

/// An address as the extension sends it; without a scheme `https://` is assumed, as in
/// haex-vault 2.x.
fn parse_address(address: &str) -> Option<Url> {
    let address = address.trim();
    if address.is_empty() {
        return None;
    }
    if address.contains("://") {
        Url::parse(address).ok()
    } else {
        Url::parse(&format!("https://{address}")).ok()
    }
}

/// The site of an address as the extension sends it.
pub(super) fn site_of_address(address: &str) -> Option<String> {
    site_of(parse_address(address)?.host_str()?)
}

pub fn get_items(call: Call<'_>) -> BoxedOutcome<'_> {
    Box::pin(get_items_async(call))
}

/// Entries for one address (FR-019 to FR-021): the headers of the scope filtered to the same site,
/// each hit read on its own, so references resolve for this caller.
// ponytail: the site filter runs in the bridge over all headers of the scope. Ceiling: one header
// list per request (SC-002 asks for under 500 ms at 5 000 entries). Upgrade path: a service method
// that filters by site, once a second caller needs it.
async fn get_items_async(call: Call<'_>) -> Outcome {
    let request: GetItems = input(call.payload)?;
    let page = request
        .url
        .as_deref()
        .and_then(parse_address)
        .ok_or(ErrorCode::InvalidRequest)?;
    let Some(site) = page.host_str().and_then(site_of) else {
        return Ok(json!({ "entries": [] }));
    };
    let grants = call.approval.grants();
    let service = service_of(call.core);
    let Headers::Items(headers) = service
        .list_headers(&call.caller, &grants)
        .await
        .map_err(to_code)?
    else {
        return Ok(json!({ "entries": [] }));
    };
    let wanted: Vec<String> = request.fields.unwrap_or_default();
    let mut entries = Vec::new();
    for header in headers {
        let same_site = header
            .url
            .as_deref()
            .and_then(site_of_address)
            .is_some_and(|candidate| candidate == site);
        if !same_site {
            continue;
        }
        let item = match service
            .read_secret_item(&call.caller, &grants, header.id.clone())
            .await
        {
            Ok(item) => item,
            // Gone or out of reach since the list: no hit.
            Err(_) => continue,
        };
        let fields = fields_of(&item);
        if !wanted.is_empty() && !wanted.iter().any(|name| fields.contains_key(name)) {
            continue;
        }
        entries.push(entry_of(item, fields, header.has_totp));
    }
    Ok(json!({ "entries": entries }))
}

/// Username, password, the custom fields and the marker `otp: "TOTP"`, all as text (FR-020). A
/// custom field never replaces one of the three.
fn fields_of(item: &SecretItem) -> Map<String, Value> {
    let mut fields = Map::new();
    for key_value in &item.key_values {
        if let (Some(key), Some(value)) = (&key_value.key, &key_value.value) {
            if !key.is_empty() {
                fields.insert(key.clone(), Value::String(value.clone()));
            }
        }
    }
    for (name, value) in [("username", &item.username), ("password", &item.password)] {
        match value {
            Some(value) if !value.is_empty() => {
                fields.insert(name.to_owned(), Value::String(value.clone()));
            }
            _ => {
                fields.remove(name);
            }
        }
    }
    if item.otp_secret.is_some() {
        fields.insert("otp".to_owned(), Value::String("TOTP".to_owned()));
    } else {
        fields.remove("otp");
    }
    fields
}

fn entry_of(item: SecretItem, fields: Map<String, Value>, has_totp: bool) -> Value {
    let aliases = item
        .autofill_aliases
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .filter(Value::is_object)
        .unwrap_or(Value::Null);
    json!({
        "id": item.id,
        "title": non_empty(item.title).unwrap_or_else(|| UNTITLED.to_owned()),
        "url": item.url,
        "fields": fields,
        "hasTotp": has_totp,
        "autofillAliases": aliases,
    })
}

pub fn get_totp(call: Call<'_>) -> BoxedOutcome<'_> {
    Box::pin(async move {
        let request: GetTotp = input(call.payload)?;
        let entry_id = non_empty(request.entry_id).ok_or(ErrorCode::InvalidRequest)?;
        let code = service_of(call.core)
            .totp_code(&call.caller, &call.approval.grants(), entry_id)
            .await
            .map_err(to_code)?;
        Ok(json!({ "code": code.code, "validFor": code.remaining_seconds }))
    })
}

pub fn create_item(call: Call<'_>) -> BoxedOutcome<'_> {
    Box::pin(async move {
        let request: CreateItem = input(call.payload)?;
        let url = non_empty(request.url);
        let title = non_empty(request.title).or_else(|| {
            url.as_deref()
                .and_then(parse_address)
                .and_then(|u| u.host_str().map(str::to_owned))
        });
        let item = ItemInput {
            title: title.clone(),
            url,
            username: non_empty(request.username),
            password: request.password.filter(|p| !p.is_empty()),
            tags: call.approval.default_tag.clone().into_iter().collect(),
            ..ItemInput::default()
        };
        let entry_id = service_of(call.core)
            .create_item(&call.caller, &call.approval.grants(), item, None)
            .await
            .map_err(to_code)?;
        Ok(json!({ "entryId": entry_id, "title": title.unwrap_or_else(|| UNTITLED.to_owned()) }))
    })
}

fn config_of(preset: &Preset) -> Value {
    json!({
        "length": preset.length,
        "uppercase": preset.uppercase,
        "lowercase": preset.lowercase,
        "numbers": preset.numbers,
        "symbols": preset.symbols,
        "excludeChars": preset.exclude_chars,
        "usePattern": preset.use_pattern,
        "pattern": preset.pattern,
    })
}

pub fn get_password_config(call: Call<'_>) -> BoxedOutcome<'_> {
    Box::pin(async move {
        let presets = service_of(call.core)
            .preset_list(&call.caller, &call.approval.grants())
            .await
            .map_err(to_code)?;
        Ok(match presets.iter().find(|p| p.is_default) {
            Some(preset) => json!({ "config": config_of(preset), "presetName": preset.name }),
            None => json!({ "config": null, "presetName": null }),
        })
    })
}

pub fn get_password_presets(call: Call<'_>) -> BoxedOutcome<'_> {
    Box::pin(async move {
        let presets = service_of(call.core)
            .preset_list(&call.caller, &call.approval.grants())
            .await
            .map_err(to_code)?;
        let presets: Vec<Value> = presets
            .iter()
            .map(|p| {
                json!({ "id": p.id, "name": p.name, "isDefault": p.is_default, "config": config_of(p) })
            })
            .collect();
        Ok(json!({ "presets": presets }))
    })
}

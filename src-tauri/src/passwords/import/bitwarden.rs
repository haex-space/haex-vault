//! Bitwarden exports (spec 034, US7, `contracts/import-mapping.md` §Bitwarden): the unencrypted
//! JSON and the CSV. Everything a source item has that holzi has no field for becomes a custom
//! field (named in German so it reads in the window) or a tag; nothing is left out. Pure functions
//! `bytes → ImportModel`.

use std::collections::HashMap;

use serde_json::Value;
use zeroize::Zeroizing;

use super::passkey::{self, RawPasskey};
use super::{
    base_state, check_otp, csv, ensure_group_path, failed, haex_time, non_empty, push_kv, push_tag,
    split_path, ImportItem, ImportModel, ImportState,
};
use crate::error::Result;
use crate::passwords::model::KeyValueInput;

const REPROMPT: &str = "Bitwarden: Passwort erneut abfragen";

/// Keys of an item that are handled by name; any other key becomes a custom field.
const KNOWN_ITEM_KEYS: [&str; 20] = [
    "id",
    "object",
    "organizationId",
    "folderId",
    "type",
    "reprompt",
    "name",
    "notes",
    "favorite",
    "fields",
    "collectionIds",
    "revisionDate",
    "creationDate",
    "deletedDate",
    "passwordHistory",
    "login",
    "secureNote",
    "card",
    "identity",
    "sshKey",
];

/// Reads an export; the JSON form starts with `{`, anything else is the CSV form.
pub fn parse(bytes: &[u8]) -> Result<ImportModel> {
    let text = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    if text.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
        parse_json(text)
    } else {
        parse_csv(bytes)
    }
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => non_empty(Some(s)),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        other => Some(other.to_string()),
    }
}

fn str_key<'a>(object: &'a Value, key: &str) -> Option<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn parse_json(bytes: &[u8]) -> Result<ImportModel> {
    let root: Value = serde_json::from_slice(bytes).map_err(|_| failed("corrupt"))?;
    if root.get("encrypted").and_then(Value::as_bool) == Some(true) {
        return Err(failed("encrypted_export"));
    }
    let Some(items) = root.get("items").and_then(Value::as_array) else {
        return Err(failed("unsupported_format"));
    };
    let mut model = ImportModel::default();
    let mut folders: HashMap<String, String> = HashMap::new();
    for folder in root
        .get("folders")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(id), Some(name)) = (str_key(folder, "id"), str_key(folder, "name")) else {
            continue;
        };
        if let Some(reference) = ensure_group_path(&mut model.groups, &split_path(name, &['/'])) {
            folders.insert(id.to_string(), reference);
        }
    }
    let mut collections: HashMap<String, String> = HashMap::new();
    for collection in root
        .get("collections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let (Some(id), Some(name)) = (str_key(collection, "id"), str_key(collection, "name")) {
            collections.insert(id.to_string(), name.to_string());
        }
    }
    for item in items {
        model.items.push(json_item(item, &folders, &collections));
    }
    Ok(model)
}

fn json_item(
    source: &Value,
    folders: &HashMap<String, String>,
    collections: &HashMap<String, String>,
) -> ImportItem {
    let mut item = ImportItem {
        title: non_empty(str_key(source, "name")),
        note: non_empty(str_key(source, "notes")),
        created_at: haex_time(str_key(source, "creationDate")),
        updated_at: haex_time(str_key(source, "revisionDate")),
        ..ImportItem::default()
    };
    if item.updated_at.is_none() {
        item.updated_at = item.created_at.clone();
    }
    let kind = source.get("type").and_then(Value::as_i64).unwrap_or(1);
    match kind {
        1 => login(&mut item, source),
        2 => {
            push_tag(&mut item.tags, "secure-note");
            if let Some(note_type) = source.pointer("/secureNote/type").and_then(Value::as_i64) {
                if note_type != 0 {
                    push_kv(
                        &mut item.key_values,
                        "Bitwarden: Notiztyp",
                        Some(&note_type.to_string()),
                    );
                }
            }
        }
        3 => {
            push_tag(&mut item.tags, "credit-card");
            let card = source.get("card").unwrap_or(&Value::Null);
            mapped(
                &mut item.key_values,
                card,
                &[
                    ("cardholderName", "Karteninhaber"),
                    ("brand", "Marke"),
                    ("number", "Kartennummer"),
                    ("expMonth", "Ablaufmonat"),
                    ("expYear", "Ablaufjahr"),
                    ("code", "Prüfnummer"),
                ],
            );
        }
        4 => {
            push_tag(&mut item.tags, "identity");
            let identity = source.get("identity").unwrap_or(&Value::Null);
            mapped(
                &mut item.key_values,
                identity,
                &[
                    ("title", "Anrede"),
                    ("firstName", "Vorname"),
                    ("middleName", "Zweiter Vorname"),
                    ("lastName", "Nachname"),
                    ("company", "Firma"),
                    ("email", "E-Mail"),
                    ("phone", "Telefon"),
                    ("address1", "Adresse 1"),
                    ("address2", "Adresse 2"),
                    ("address3", "Adresse 3"),
                    ("city", "Ort"),
                    ("state", "Bundesland oder Region"),
                    ("postalCode", "Postleitzahl"),
                    ("country", "Land"),
                    ("ssn", "Sozialversicherungsnummer"),
                    ("passportNumber", "Passnummer"),
                    ("licenseNumber", "Führerscheinnummer"),
                    ("username", "Benutzername"),
                ],
            );
        }
        5 => {
            push_tag(&mut item.tags, "ssh-key");
            let key = source.get("sshKey").unwrap_or(&Value::Null);
            mapped(
                &mut item.key_values,
                key,
                &[
                    ("privateKey", "SSH privater Schlüssel"),
                    ("publicKey", "SSH öffentlicher Schlüssel"),
                    ("keyFingerprint", "SSH Fingerabdruck"),
                ],
            );
        }
        other => push_tag(&mut item.tags, &format!("Bitwarden-Typ {other}")),
    }
    custom_fields(&mut item, source);
    if source.get("favorite").and_then(Value::as_bool) == Some(true) {
        push_tag(&mut item.tags, "Favorit");
    }
    if source.get("reprompt").and_then(Value::as_i64).unwrap_or(0) != 0 {
        push_kv(&mut item.key_values, REPROMPT, Some("ja"));
    }
    let mut collected = false;
    for id in source
        .get("collectionIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(id) = id.as_str() {
            let name = collections.get(id).map(String::as_str).unwrap_or(id);
            push_tag(&mut item.tags, &format!("Sammlung: {name}"));
            collected = true;
        }
    }
    if !collected {
        if let Some(org) = str_key(source, "organizationId") {
            push_tag(&mut item.tags, &format!("Sammlung: {org}"));
        }
    }
    // Properties no field was found for stay as custom fields (an unknown type keeps all of them).
    if let Some(object) = source.as_object() {
        for (key, value) in object {
            if !KNOWN_ITEM_KEYS.contains(&key.as_str()) {
                push_kv(
                    &mut item.key_values,
                    &format!("Bitwarden: {key}"),
                    text_of(value).as_deref(),
                );
            }
        }
    }
    let folder = str_key(source, "folderId")
        .and_then(|id| folders.get(id))
        .cloned();
    if non_empty(str_key(source, "deletedDate")).is_some() {
        item.trashed = true;
        item.trashed_from_ref = folder;
    } else {
        item.group_ref = folder;
    }
    history(&mut item, source);
    check_otp(&mut item);
    item
}

fn mapped(fields: &mut Vec<KeyValueInput>, object: &Value, names: &[(&str, &str)]) {
    for (key, label) in names {
        push_kv(fields, label, object.get(*key).and_then(text_of).as_deref());
    }
    // A property of the object that is not in the table stays as it is.
    if let Some(map) = object.as_object() {
        for (key, value) in map {
            if !names.iter().any(|(known, _)| known == key) {
                push_kv(
                    fields,
                    &format!("Bitwarden: {key}"),
                    text_of(value).as_deref(),
                );
            }
        }
    }
}

fn login(item: &mut ImportItem, source: &Value) {
    let login = source.get("login").unwrap_or(&Value::Null);
    item.username = non_empty(str_key(login, "username"));
    item.password = non_empty(str_key(login, "password"));
    item.otp_raw = non_empty(str_key(login, "totp"));
    let uris = login
        .get("uris")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for (index, uri) in uris.iter().enumerate() {
        let address = str_key(uri, "uri");
        let matching = uri.get("match").and_then(text_of);
        if index == 0 {
            item.url = non_empty(address);
            push_kv(&mut item.key_values, "URL Zuordnung", matching.as_deref());
        } else {
            let number = index + 1;
            push_kv(&mut item.key_values, &format!("URL {number}"), address);
            push_kv(
                &mut item.key_values,
                &format!("URL {number} Zuordnung"),
                matching.as_deref(),
            );
        }
    }
    for credential in login
        .get("fido2Credentials")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(raw) = fido2(credential) {
            let passkey = passkey::build(raw, &mut item.problems);
            item.passkeys.push(passkey);
        }
    }
    if let Some(map) = login.as_object() {
        for (key, value) in map {
            if !["username", "password", "uris", "totp", "fido2Credentials"].contains(&key.as_str())
            {
                push_kv(
                    &mut item.key_values,
                    &format!("Bitwarden: {key}"),
                    text_of(value).as_deref(),
                );
            }
        }
    }
}

/// A `fido2Credentials` entry. The credential id is a UUID text that becomes its 16 bytes; a text
/// that is no UUID is taken as Base64 of the id as it is.
fn fido2(credential: &Value) -> Option<RawPasskey> {
    let id_text = str_key(credential, "credentialId")?;
    let credential_id = uuid_bytes(id_text)
        .map(|bytes| base64_standard(&bytes))
        .or_else(|| passkey::decode_base64_any(id_text).map(|b| base64_standard(&b)))
        .unwrap_or_else(|| id_text.to_string());
    let algorithm = match (
        str_key(credential, "keyAlgorithm"),
        str_key(credential, "keyCurve"),
    ) {
        (Some(a), Some(c))
            if a.eq_ignore_ascii_case("ECDSA") && c.eq_ignore_ascii_case("P-256") =>
        {
            Some(-7)
        }
        (Some(a), _) if a.eq_ignore_ascii_case("ES256") => Some(-7),
        (Some(a), _) if a.eq_ignore_ascii_case("EdDSA") || a.eq_ignore_ascii_case("Ed25519") => {
            Some(-8)
        }
        (Some(a), _) if a.eq_ignore_ascii_case("RS256") || a.eq_ignore_ascii_case("RSA") => {
            Some(-257)
        }
        (Some(_), _) => Some(0),
        (None, _) => None,
    };
    Some(RawPasskey {
        credential_id,
        relying_party_id: str_key(credential, "rpId")?.to_string(),
        relying_party_name: non_empty(str_key(credential, "rpName")),
        user_name: non_empty(str_key(credential, "userName")),
        user_display_name: non_empty(str_key(credential, "userDisplayName")),
        user_handle: str_key(credential, "userHandle").unwrap_or("").to_string(),
        private_key: Zeroizing::new(str_key(credential, "keyValue").unwrap_or("").to_string()),
        algorithm,
        sign_count: str_key(credential, "counter")
            .and_then(|c| c.parse().ok())
            .unwrap_or(0),
        is_discoverable: str_key(credential, "discoverable") != Some("false"),
        created_at: haex_time(str_key(credential, "creationDate")),
    })
}

fn base64_standard(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The 16 bytes of a UUID text (`xxxxxxxx-xxxx-…`), `None` for anything else.
fn uuid_bytes(text: &str) -> Option<[u8; 16]> {
    uuid::Uuid::parse_str(text).ok().map(|u| *u.as_bytes())
}

/// Fields of the item (`fields[]`): text, hidden, yes/no and linked.
fn custom_fields(item: &mut ImportItem, source: &Value) {
    for field in source
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = str_key(field, "name").unwrap_or("Feld");
        let kind = field.get("type").and_then(Value::as_i64).unwrap_or(0);
        let value = if kind == 3 {
            field
                .get("linkedId")
                .and_then(text_of)
                .map(|target| format!("Verknüpft: {target}"))
        } else {
            field.get("value").and_then(text_of)
        };
        push_kv(&mut item.key_values, name, value.as_deref());
    }
}

/// `passwordHistory[]` as earlier states with the times of the source.
fn history(item: &mut ImportItem, source: &Value) {
    let mut states: Vec<ImportState> = Vec::new();
    for entry in source
        .get("passwordHistory")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(password) = str_key(entry, "password") else {
            continue;
        };
        let mut data = base_state(item);
        data.password = Some(password.to_string());
        states.push(ImportState {
            modified_at: haex_time(str_key(entry, "lastUsedDate"))
                .or_else(|| item.created_at.clone()),
            data,
            attachments: Vec::new(),
        });
    }
    states.sort_by(|a, b| a.modified_at.cmp(&b.modified_at));
    item.history = states;
}

// --- CSV -------------------------------------------------------------------------------------

fn parse_csv(bytes: &[u8]) -> Result<ImportModel> {
    let table = csv::parse(bytes)?;
    if !table.has_column("name") || !table.has_column("login_username") {
        return Err(failed("unsupported_format"));
    }
    let mut model = ImportModel::default();
    for row in &table.rows {
        let cell = |column: &str| table.cell(row, column).and_then(|v| non_empty(Some(v)));
        let mut item = ImportItem {
            title: cell("name"),
            note: cell("notes"),
            username: cell("login_username"),
            password: cell("login_password"),
            otp_raw: cell("login_totp"),
            ..ImportItem::default()
        };
        if cell("type").as_deref() == Some("note") {
            push_tag(&mut item.tags, "secure-note");
        }
        for (index, address) in cell("login_uri")
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .enumerate()
        {
            if index == 0 {
                item.url = Some(address.trim().to_string());
            } else {
                push_kv(
                    &mut item.key_values,
                    &format!("URL {}", index + 1),
                    Some(address.trim()),
                );
            }
        }
        for line in cell("fields").unwrap_or_default().lines() {
            if let Some((name, value)) = line.split_once(": ") {
                push_kv(&mut item.key_values, name.trim(), Some(value));
            } else if let Some((name, value)) = line.split_once(':') {
                push_kv(&mut item.key_values, name.trim(), Some(value.trim()));
            }
        }
        if cell("favorite").as_deref() == Some("1") {
            push_tag(&mut item.tags, "Favorit");
        }
        if cell("reprompt").is_some_and(|r| r != "0") {
            push_kv(&mut item.key_values, REPROMPT, Some("ja"));
        }
        if let Some(folder) = cell("folder") {
            item.group_ref = ensure_group_path(&mut model.groups, &split_path(&folder, &['/']));
        }
        check_otp(&mut item);
        model.items.push(item);
    }
    Ok(model)
}

#[cfg(test)]
#[path = "bitwarden_tests.rs"]
mod tests;

//! KeePass (KDBX) databases (spec 034, US7, `contracts/import-mapping.md` §KeePass), read with the
//! `keepass` crate (KDBX 3 and 4; password and key file). The recycle bin becomes the trash, the
//! history becomes earlier states with their attachments, standard icons map to pictures of haex-vault
//! and custom icons stay as pictures. Everything without a field of its own (colours, override URL,
//! Auto-Type, custom data, custom fields) becomes a custom field. Pure: `bytes → ImportModel`.

use std::io::Cursor;

use keepass::db::{
    CustomDataValue, DataTransferObfuscation, Database, DatabaseOpenError, EntryRef, GroupRef, Icon,
};
use keepass::DatabaseKey;
use zeroize::Zeroizing;

use super::icons::standard_icon;
use super::passkey::{self, RawPasskey};
use super::{
    base_state, check_otp, failed, non_empty, push_kv, Credentials, IconRef, ImportAttachment,
    ImportGroup, ImportItem, ImportModel, ImportState, Problem,
};
use crate::error::Result;
use crate::passwords::model::{AttentionKind, KeyValueInput};

/// Fields of an entry that are read by name and so are not custom fields.
const NAMED_FIELDS: [&str; 9] = [
    "Title",
    "UserName",
    "Password",
    "URL",
    "Notes",
    "otp",
    "OTP",
    "TOTP Seed",
    "TOTP Settings",
];
const PASSKEY_PREFIX: &str = "KPEX_PASSKEY_";

/// Reads a database. A wrong password or key file is `wrong_credentials`, a damaged or foreign file
/// `corrupt`, a format the crate does not read `unsupported_format`.
pub fn parse(bytes: &[u8], credentials: &Credentials) -> Result<ImportModel> {
    let mut key = DatabaseKey::new();
    if let Some(password) = &credentials.password {
        key = key.with_password(password);
    }
    if let Some(file) = &credentials.key_file {
        key = key
            .with_keyfile(&mut Cursor::new(file.as_slice()))
            .map_err(|_| failed("unreadable"))?;
    }
    if key.is_empty() {
        return Err(failed("wrong_credentials"));
    }
    let db = Database::parse(bytes, key).map_err(|error| match error {
        DatabaseOpenError::Key(_) => failed("wrong_credentials"),
        DatabaseOpenError::Io(_) => failed("unreadable"),
        DatabaseOpenError::UnsupportedVersion => failed("unsupported_format"),
        _ => failed("corrupt"),
    })?;
    let recycle_bin = db.meta.recyclebin_uuid.map(|id| id.to_string());
    let mut reader = Reader {
        model: ImportModel::default(),
        recycle_bin,
        settings_seen: false,
    };
    reader.walk(&db.root(), None, false);
    if reader.settings_seen {
        reader
            .model
            .source_problems
            .push(Problem::new(AttentionKind::SourceSetting));
    }
    Ok(reader.model)
}

struct Reader {
    model: ImportModel,
    recycle_bin: Option<String>,
    /// A group or entry carries a setting of the application (search or Auto-Type per group,
    /// quality check) that haex-vault has no use for; it is reported once.
    settings_seen: bool,
}

/// A time of the database as the vault stores it. A macro because the `chrono` type is not named
/// here (the crate is not a direct dependency).
macro_rules! stamp {
    ($time:expr) => {
        $time.map(|t| crate::passwords::clock::format_millis(t.and_utc().timestamp_millis()))
    };
}

fn color_text(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

impl Reader {
    /// Reads a group and, below it, its groups and entries. `in_bin` is true inside the recycle bin.
    fn walk(&mut self, group: &GroupRef<'_>, parent_ref: Option<&str>, in_bin: bool) {
        let reference = group.id().to_string();
        let is_root = parent_ref.is_none() && group.parent().is_none();
        let is_bin = self.recycle_bin.as_deref() == Some(reference.as_str());
        if !is_root {
            let mut problems = Vec::new();
            let icon = group_icon(group, &mut problems);
            self.model.source_problems.extend(problems);
            self.model.groups.push(ImportGroup {
                reference: reference.clone(),
                parent_ref: parent_ref.map(str::to_string),
                name: group.name.clone(),
                description: non_empty(group.notes.as_deref()),
                icon,
                is_recycle_bin: is_bin,
                previous_parent_ref: group.previous_parent().map(|g| g.id().to_string()),
                color: None,
                sort_order: None,
            });
        }
        if group.enable_searching.is_some() || group.enable_autotype.is_some() {
            self.settings_seen = true;
        }
        let in_bin = in_bin || is_bin;
        let own_ref = if is_root {
            None
        } else {
            Some(reference.as_str())
        };
        for entry in group.entries() {
            if !entry.quality_check {
                self.settings_seen = true;
            }
            let mut item = read_entry(&entry);
            item.source_ref = Some(entry.id().to_string());
            item.history = read_history(&entry);
            item.group_ref = own_ref.map(str::to_string);
            item.trashed = in_bin;
            if in_bin {
                item.trashed_from_ref = entry.previous_parent().map(|g| g.id().to_string());
            }
            check_otp(&mut item);
            self.model.items.push(item);
        }
        for child in group.groups() {
            self.walk(&child, own_ref, in_bin);
        }
    }
}

fn group_icon(group: &GroupRef<'_>, problems: &mut Vec<Problem>) -> Option<IconRef> {
    if let Some(custom) = group.custom_icon() {
        return Some(IconRef::Custom(custom.data.clone()));
    }
    match group.icon() {
        Some(Icon::BuiltIn(index)) => match standard_icon(*index) {
            Some(name) => Some(IconRef::Standard(name.to_string())),
            None => {
                problems.push(Problem::field(AttentionKind::IconNotMapped, "icon"));
                None
            }
        },
        _ => None,
    }
}

fn entry_icon(entry: &EntryRef<'_>, problems: &mut Vec<Problem>) -> Option<IconRef> {
    if let Some(custom) = entry.custom_icon() {
        return Some(IconRef::Custom(custom.data.clone()));
    }
    match entry.icon() {
        Some(Icon::BuiltIn(index)) => match standard_icon(*index) {
            Some(name) => Some(IconRef::Standard(name.to_string())),
            None => {
                problems.push(Problem::field(AttentionKind::IconNotMapped, "icon"));
                None
            }
        },
        _ => None,
    }
}

/// The first `otpauth://` address in a text, up to the next white space.
fn otpauth_in(text: &str) -> Option<String> {
    let start = text.find("otpauth://")?;
    let rest = &text[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// `TOTP Settings` as `period;digits` (a number for each); anything else, such as Steam's `S`, is
/// not a setting haex-vault can reproduce.
fn totp_settings(text: &str) -> Option<(i64, i64)> {
    let mut parts = text.split(';');
    let period = parts.next()?.trim().parse().ok()?;
    let digits = parts.next()?.trim().parse().ok()?;
    Some((period, digits))
}

fn read_totp(entry: &EntryRef<'_>, item: &mut ImportItem) {
    let field = |name: &str| non_empty(entry.get(name));
    if let Some(raw) = field("otp").or_else(|| field("OTP")) {
        item.otp_raw = Some(raw);
    } else if let Some(seed) = field("TOTP Seed") {
        item.otp_raw = Some(seed);
        if let Some(settings) = field("TOTP Settings") {
            match totp_settings(&settings) {
                Some((period, digits)) => {
                    item.otp_period = Some(period);
                    item.otp_digits = Some(digits);
                }
                None => item
                    .problems
                    .push(Problem::field(AttentionKind::TotpInvalid, "TOTP Settings")),
            }
        }
    } else if let Some(address) = entry.get("Notes").and_then(otpauth_in) {
        item.otp_raw = Some(address);
    }
}

fn read_passkey(
    entry: &EntryRef<'_>,
    problems: &mut Vec<Problem>,
) -> Option<crate::passwords::passkeys::PasskeyInput> {
    let field = |name: &str| non_empty(entry.get(&format!("{PASSKEY_PREFIX}{name}")));
    let id_text = field("CREDENTIAL_ID")?;
    let credential_id = passkey::decode_base64_any(&id_text)
        .map(|bytes| {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(bytes)
        })
        .unwrap_or(id_text);
    let pem = field("PRIVATE_KEY_PEM").unwrap_or_default();
    let raw = RawPasskey {
        credential_id,
        relying_party_id: field("RELYING_PARTY")?,
        relying_party_name: None,
        user_name: field("USERNAME"),
        user_display_name: None,
        user_handle: field("USER_HANDLE").unwrap_or_default(),
        private_key: Zeroizing::new(passkey::strip_pem(&pem)),
        algorithm: None,
        sign_count: 0,
        is_discoverable: true,
        created_at: None,
    };
    Some(passkey::build(raw, problems))
}

fn read_attachments(entry: &EntryRef<'_>, item: &mut ImportItem) -> Vec<ImportAttachment> {
    let mut named: Vec<(String, Vec<u8>)> = entry
        .attachments_named()
        .map(|(name, attachment)| (name.to_string(), attachment.data.get().clone()))
        .collect();
    named.sort_by(|a, b| a.0.cmp(&b.0));
    let mut kept = Vec::new();
    for (name, bytes) in named {
        match ImportAttachment::within_limit(&name, bytes) {
            Ok(attachment) => kept.push(attachment),
            Err(problem) => item.problems.push(problem),
        }
    }
    kept
}

/// One entry (or one historical state of it) as an item. Group, trash and history are set by the
/// caller.
fn read_entry(entry: &EntryRef<'_>) -> ImportItem {
    let mut item = ImportItem {
        title: non_empty(entry.get("Title")),
        username: non_empty(entry.get("UserName")),
        password: non_empty(entry.get("Password")),
        url: non_empty(entry.get("URL")),
        note: non_empty(entry.get("Notes")),
        created_at: stamp!(entry.times.creation),
        updated_at: stamp!(entry.times.last_modification),
        tags: entry
            .tags
            .iter()
            .filter(|t| !t.trim().is_empty())
            .cloned()
            .collect(),
        ..ImportItem::default()
    };
    if entry.times.expires == Some(true) {
        item.expires_at = entry.times.expiry.map(|t| t.date().to_string());
    }
    read_totp(entry, &mut item);
    let mut names: Vec<&String> = entry
        .fields
        .keys()
        .filter(|k| !NAMED_FIELDS.contains(&k.as_str()) && !k.starts_with(PASSKEY_PREFIX))
        .collect();
    names.sort();
    for name in names {
        let value = entry
            .get(name)
            .map(str::to_string)
            .filter(|v| !v.is_empty());
        item.key_values.push(KeyValueInput {
            key: name.clone(),
            value,
        });
    }
    extras(entry, &mut item.key_values);
    if let Some(passkey) = read_passkey(entry, &mut item.problems) {
        item.passkeys.push(passkey);
    }
    item.icon = entry_icon(entry, &mut item.problems);
    item.attachments = read_attachments(entry, &mut item);
    item
}

/// Colours, override URL, Auto-Type and custom data: no field of haex-vault, so custom fields.
fn extras(entry: &EntryRef<'_>, fields: &mut Vec<KeyValueInput>) {
    if let Some(c) = &entry.foreground_color {
        push_kv(
            fields,
            "KeePass: Vordergrundfarbe",
            Some(&color_text(c.r, c.g, c.b)),
        );
    }
    if let Some(c) = &entry.background_color {
        push_kv(
            fields,
            "KeePass: Hintergrundfarbe",
            Some(&color_text(c.r, c.g, c.b)),
        );
    }
    push_kv(
        fields,
        "KeePass: URL überschreiben",
        entry.override_url.as_deref(),
    );
    if let Some(autotype) = &entry.autotype {
        push_kv(
            fields,
            "KeePass: Auto-Type aktiv",
            Some(if autotype.enabled { "ja" } else { "nein" }),
        );
        push_kv(
            fields,
            "KeePass: Auto-Type Standardfolge",
            autotype.default_sequence.as_deref(),
        );
        if autotype.data_transfer_obfuscation == DataTransferObfuscation::UseClipboard {
            push_kv(
                fields,
                "KeePass: Auto-Type Verschleierung",
                Some("Zwischenablage"),
            );
        }
        for (index, association) in autotype.associations.iter().enumerate() {
            let number = index + 1;
            push_kv(
                fields,
                &format!("KeePass: Auto-Type Fenster {number}"),
                Some(&association.window),
            );
            push_kv(
                fields,
                &format!("KeePass: Auto-Type Fenster {number} Folge"),
                Some(&association.sequence),
            );
        }
    }
    let mut keys: Vec<&String> = entry.custom_data.keys().collect();
    keys.sort();
    for key in keys {
        let text = match entry
            .custom_data
            .get(key)
            .and_then(|item| item.value.as_ref())
        {
            Some(CustomDataValue::String(s)) => Some(s.clone()),
            Some(CustomDataValue::Binary(bytes)) => {
                use base64::Engine;
                Some(base64::engine::general_purpose::STANDARD.encode(bytes))
            }
            None => None,
        };
        push_kv(
            fields,
            &format!("KeePass: Zusatzdaten {key}"),
            text.as_deref(),
        );
    }
}

/// The earlier states of an entry, oldest first, each with its own attachments.
fn read_history(entry: &EntryRef<'_>) -> Vec<ImportState> {
    let mut states = Vec::new();
    let mut index = 0;
    while let Some(old) = entry.historical(index) {
        let mut state_item = read_entry(&old);
        let mut data = base_state(&state_item);
        data.icon = match &state_item.icon {
            Some(IconRef::Standard(name)) => Some(name.clone()),
            _ => None,
        };
        states.push(ImportState {
            modified_at: state_item.updated_at.take(),
            data,
            attachments: std::mem::take(&mut state_item.attachments),
        });
        index += 1;
    }
    states.sort_by(|a, b| a.modified_at.cmp(&b.modified_at));
    states
}

//! Revealing, copying and the TOTP code (spec 034, FR-003, FR-005, FR-006, research R7): the only
//! functions through which the value of a secret leaves the entry's row. The window gets a value
//! only by an explicit act of the user (`reveal`) or not at all (`copy_value` goes to the clipboard
//! in Rust, `totp_code` returns the code, never the secret).

use std::fmt;

use haex_crdt::rusqlite::params;
use zeroize::Zeroizing;

use super::items::otp_params_of;
use super::model::{
    CopyField, KeyValuePatch, RevealedSecret, SecretField, SecretItem, SecretKeyValue, TotpCode,
};
use super::references::Field;
use super::references_db::{resolve_or_error, resolve_value, Reader};
use super::totp::{code_at, remaining_seconds};
use crate::error::{HaexError, Result};
use crate::storage::query::Query;

/// A value on its way to the clipboard. It is zeroed when dropped and prints no value.
pub struct Copied(Zeroizing<String>);

impl Copied {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> Zeroizing<String> {
        self.0
    }
}

impl fmt::Debug for Copied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Copied(<redacted>)")
    }
}

/// One column of an entry as text; an empty cell reads as an empty string, a missing entry is
/// `PasswordsNotFound`.
fn column(q: &mut impl Query, item_id: &str, column: &'static str) -> Result<Zeroizing<String>> {
    // `column` is one of the literals below, never input.
    let sql = format!("SELECT {column} FROM haex_passwords_item_details WHERE id = ?1");
    let cell = q
        .query_row(&sql, params![item_id], |r| r.get::<_, Option<String>>(0))?
        .ok_or(HaexError::PasswordsNotFound)?;
    Ok(Zeroizing::new(cell.unwrap_or_default()))
}

/// A custom field's key and value. A stored row may have no key (no placeholder can reach it);
/// it reads as the empty key, which the grammar never names.
fn key_value(
    q: &mut impl Query,
    item_id: &str,
    field_id: &str,
) -> Result<(String, Zeroizing<String>)> {
    let (key, cell) = q
        .query_row(
            "SELECT key, value FROM haex_passwords_item_key_values WHERE id = ?1 AND item_id = ?2",
            params![field_id, item_id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            },
        )?
        .ok_or(HaexError::PasswordsNotFound)?;
    Ok((
        key.unwrap_or_default(),
        Zeroizing::new(cell.unwrap_or_default()),
    ))
}

/// A field of the entry with its placeholders resolved for the user (spec 036, FR-045): a
/// placeholder that does not resolve is the error, never the text.
fn resolved_column(
    q: &mut impl Query,
    item_id: &str,
    name: &'static str,
    field: Field,
) -> Result<Zeroizing<String>> {
    let raw = column(q, item_id, name)?;
    resolve_or_error(q, Reader::user(), item_id, field, &raw)
}

fn resolved_key_value(
    q: &mut impl Query,
    item_id: &str,
    field_id: &str,
) -> Result<Zeroizing<String>> {
    let (key, raw) = key_value(q, item_id, field_id)?;
    resolve_or_error(q, Reader::user(), item_id, Field::Extra(key), &raw)
}

/// The value of a secret field, or the resolved value of a text field with references, for the
/// moment the user asks to see it. The TOTP secret holds no references (FR-044).
pub fn reveal(q: &mut impl Query, item_id: &str, field: &SecretField) -> Result<RevealedSecret> {
    let value = match field {
        SecretField::Password => resolved_column(q, item_id, "password", Field::Password)?,
        SecretField::OtpSecret => column(q, item_id, "otp_secret")?,
        SecretField::KeyValue { id } => resolved_key_value(q, item_id, id)?,
        SecretField::Username => resolved_column(q, item_id, "username", Field::Username)?,
        SecretField::Url => resolved_column(q, item_id, "url", Field::Url)?,
        SecretField::Note => resolved_column(q, item_id, "note", Field::Note)?,
        SecretField::StoredPassword => column(q, item_id, "password")?,
    };
    Ok(RevealedSecret { value })
}

/// The value to put on the clipboard; for `Totp` the current code of this moment.
pub fn copy_value(q: &mut impl Query, item_id: &str, field: &CopyField) -> Result<Copied> {
    let value = match field {
        CopyField::Username => resolved_column(q, item_id, "username", Field::Username)?,
        CopyField::Password => resolved_column(q, item_id, "password", Field::Password)?,
        CopyField::KeyValue { id } => resolved_key_value(q, item_id, id)?,
        CopyField::Totp => Zeroizing::new(totp_code(q, item_id, unix_now())?.code),
        CopyField::Url => resolved_column(q, item_id, "url", Field::Url)?,
        CopyField::Note => resolved_column(q, item_id, "note", Field::Note)?,
    };
    Ok(Copied(value))
}

/// Resolves the placeholders of a whole entry for a caller from outside (spec 036, FR-047): a field
/// whose placeholder does not resolve for this caller (missing source, source outside its scope or
/// in the trash, cycle, too deep) is left out of the answer, with no hint why.
pub fn resolve_secret_item(
    q: &mut impl Query,
    reader: Reader<'_>,
    item: &mut SecretItem,
) -> Result<()> {
    let id = item.id.clone();
    for (field, value) in [
        (Field::Username, &mut item.username),
        (Field::Password, &mut item.password),
        (Field::Url, &mut item.url),
        (Field::Note, &mut item.note),
    ] {
        if let Some(raw) = value.take() {
            *value = resolve_value(q, reader, &id, field, &raw)?
                .ok()
                .map(|resolved| resolved.to_string());
        }
    }
    let mut kept = Vec::with_capacity(item.key_values.len());
    for entry in item.key_values.drain(..) {
        kept.extend(visible_key_value(q, reader, &id, entry)?);
    }
    item.key_values = kept;
    Ok(())
}

/// A custom field as `reader` sees it: resolved, or `None` when its placeholder does not resolve
/// for this caller.
fn visible_key_value(
    q: &mut impl Query,
    reader: Reader<'_>,
    item_id: &str,
    entry: SecretKeyValue,
) -> Result<Option<SecretKeyValue>> {
    let Some(raw) = entry.value.as_deref() else {
        return Ok(Some(entry));
    };
    // A field without a key is resolved too (as the empty key, which no placeholder names), so its
    // placeholder never leaves raw.
    let field = Field::Extra(entry.key.clone().unwrap_or_default());
    Ok(resolve_value(q, reader, item_id, field, raw)?
        .ok()
        .map(|resolved| SecretKeyValue {
            id: entry.id,
            key: entry.key,
            value: Some(resolved.to_string()),
        }))
}

/// The custom fields [`resolve_secret_item`] leaves out for `reader`, as patch entries that keep
/// them unchanged: a caller from outside that replaces the custom fields cannot delete one it never
/// saw (spec 036, FR-047).
pub fn hidden_key_values(
    q: &mut impl Query,
    reader: Reader<'_>,
    item_id: &str,
) -> Result<Vec<KeyValuePatch>> {
    let mut hidden = Vec::new();
    for entry in stored_key_values(q, item_id)? {
        let (id, key) = (entry.id.clone(), entry.key.clone());
        if visible_key_value(q, reader, item_id, entry)?.is_none() {
            hidden.push(KeyValuePatch {
                id: Some(id),
                key: key.unwrap_or_default(),
                value: None,
            });
        }
    }
    Ok(hidden)
}

/// The code of the entry at a Unix time with the seconds until it changes. `PasswordsNotFound` for
/// a missing entry, `InvalidInput` for a missing or invalid secret, digits, period or algorithm.
pub fn totp_code(q: &mut impl Query, item_id: &str, unix_seconds: u64) -> Result<TotpCode> {
    let params = otp_params_of(q, item_id)?;
    Ok(TotpCode {
        code: code_at(&params, unix_seconds),
        remaining_seconds: remaining_seconds(params.period, unix_seconds),
        period: params.period,
        digits: params.digits,
    })
}

/// The system time in whole seconds; a clock before 1970 reads as 0.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The whole entry with its secrets for the single-entry read of callers from outside; `None` for a
/// missing entry. The access rules decide before this is called.
pub fn secret_item(q: &mut impl Query, item_id: &str) -> Result<Option<SecretItem>> {
    let Some(mut item) = q.query_row(
        "SELECT id, title, username, password, note, url, expires_at, otp_secret, otp_digits, \
                otp_period, otp_algorithm, icon, color, autofill_aliases, created_at, updated_at \
         FROM haex_passwords_item_details WHERE id = ?1",
        params![item_id],
        |r| {
            Ok(SecretItem {
                id: r.get(0)?,
                title: r.get(1)?,
                username: r.get(2)?,
                password: r.get(3)?,
                note: r.get(4)?,
                url: r.get(5)?,
                expires_at: r.get(6)?,
                otp_secret: r.get(7)?,
                otp_digits: r
                    .get::<_, Option<i64>>(8)?
                    .and_then(|v| u32::try_from(v).ok()),
                otp_period: r
                    .get::<_, Option<i64>>(9)?
                    .and_then(|v| u32::try_from(v).ok()),
                otp_algorithm: r.get(10)?,
                icon: r.get(11)?,
                color: r.get(12)?,
                autofill_aliases: r.get(13)?,
                created_at: r.get(14)?,
                updated_at: r.get(15)?,
                tags: Vec::new(),
                key_values: Vec::new(),
            })
        },
    )?
    else {
        return Ok(None);
    };
    item.tags = super::tags::names_of_item(q, item_id)?;
    item.key_values = stored_key_values(q, item_id)?;
    Ok(Some(item))
}

fn stored_key_values(q: &mut impl Query, item_id: &str) -> Result<Vec<SecretKeyValue>> {
    Ok(q.query_map(
        "SELECT id, key, value FROM haex_passwords_item_key_values WHERE item_id = ?1 \
         ORDER BY rowid",
        params![item_id],
        |r| {
            Ok(SecretKeyValue {
                id: r.get(0)?,
                key: r.get(1)?,
                value: r.get(2)?,
            })
        },
    )?)
}

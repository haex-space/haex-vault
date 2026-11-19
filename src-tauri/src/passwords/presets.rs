//! Generator presets (spec 034, US3, FR-014): saved configurations of the password generator and
//! exactly one default. The generating itself happens in the window (research R10); this is only
//! the storage. Setting a default clears the flag on every other preset in the same write; if two
//! devices set a default at once, the list shows only the newest as the default.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::clock;
use super::model::{Preset, PresetInput};
use crate::error::{HaexError, Result};
use crate::storage::query::Query;

const MAX_LENGTH: u32 = 256;

fn invalid(reason: &str) -> HaexError {
    HaexError::InvalidInput {
        reason: reason.to_string(),
    }
}

/// All presets by name; of several flagged defaults only the one with the newest `updated_at`
/// keeps the flag.
pub fn list(q: &mut impl Query) -> Result<Vec<Preset>> {
    let rows: Vec<(Preset, String)> = q.query_map(
        "SELECT id, name, length, uppercase, lowercase, numbers, symbols, exclude_chars, \
                use_pattern, pattern, is_default, updated_at \
         FROM haex_passwords_generator_presets ORDER BY name COLLATE NOCASE, id",
        &[],
        |r| {
            Ok((
                Preset {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    length: r.get::<_, i64>(2)?.clamp(1, i64::from(MAX_LENGTH)) as u32,
                    uppercase: r.get::<_, i64>(3)? != 0,
                    lowercase: r.get::<_, i64>(4)? != 0,
                    numbers: r.get::<_, i64>(5)? != 0,
                    symbols: r.get::<_, i64>(6)? != 0,
                    exclude_chars: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                    use_pattern: r.get::<_, i64>(8)? != 0,
                    pattern: r.get::<_, Option<String>>(9)?.unwrap_or_default(),
                    is_default: r.get::<_, i64>(10)? != 0,
                },
                r.get::<_, Option<String>>(11)?.unwrap_or_default(),
            ))
        },
    )?;
    let newest_default = rows
        .iter()
        .filter(|(p, _)| p.is_default)
        .max_by(|(_, a), (_, b)| a.cmp(b))
        .map(|(p, _)| p.id.clone());
    Ok(rows
        .into_iter()
        .map(|(mut p, _)| {
            p.is_default = newest_default.as_deref() == Some(p.id.as_str());
            p
        })
        .collect())
}

/// Saves a preset: an empty `id` creates one. The name may not be empty after trim, the length is
/// 1–256. With `is_default` every other preset loses the flag in the same write.
pub fn save(tx: &mut CrdtTransaction<'_>, input: &PresetInput) -> Result<String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name"));
    }
    if input.length == 0 || input.length > MAX_LENGTH {
        return Err(invalid("length"));
    }
    let now = clock::now();
    let id = if input.id.is_empty() {
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO haex_passwords_generator_presets \
             (id, name, length, uppercase, lowercase, numbers, symbols, exclude_chars, \
              use_pattern, pattern, is_default, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
            params![
                id,
                name,
                input.length,
                i64::from(input.uppercase),
                i64::from(input.lowercase),
                i64::from(input.numbers),
                i64::from(input.symbols),
                input.exclude_chars,
                i64::from(input.use_pattern),
                input.pattern,
                i64::from(input.is_default),
                now,
            ],
        )?;
        id
    } else {
        let changed = tx.execute(
            "UPDATE haex_passwords_generator_presets \
             SET name = ?1, length = ?2, uppercase = ?3, lowercase = ?4, numbers = ?5, \
                 symbols = ?6, exclude_chars = ?7, use_pattern = ?8, pattern = ?9, \
                 is_default = ?10, updated_at = ?11 WHERE id = ?12",
            params![
                name,
                input.length,
                i64::from(input.uppercase),
                i64::from(input.lowercase),
                i64::from(input.numbers),
                i64::from(input.symbols),
                input.exclude_chars,
                i64::from(input.use_pattern),
                input.pattern,
                i64::from(input.is_default),
                now,
                input.id,
            ],
        )?;
        if changed == 0 {
            return Err(HaexError::PasswordsNotFound);
        }
        input.id.clone()
    };
    if input.is_default {
        tx.execute(
            "UPDATE haex_passwords_generator_presets SET is_default = 0, updated_at = ?1 \
             WHERE is_default = 1 AND id <> ?2",
            params![now, id],
        )?;
    }
    Ok(id)
}

/// Deletes one preset.
pub fn delete(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<()> {
    let removed = tx.execute(
        "DELETE FROM haex_passwords_generator_presets WHERE id = ?1",
        params![id],
    )?;
    if removed == 0 {
        return Err(HaexError::PasswordsNotFound);
    }
    Ok(())
}

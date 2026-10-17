//! What an import writes besides folders, entries and attachments (spec 037, research R4): the
//! colours of tags, passkeys that belong to no entry, and presets of the password generator. Only
//! haex-vault 2.x (Legacy) delivers these today; for the other sources the lists are empty and nothing happens.
//! They are few and small, so they go in one write after the entries.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::Problem;
use crate::error::{HolziError, Result};
use crate::passwords::ids::fold;
use crate::passwords::model::{AttentionKind, PresetInput};
use crate::passwords::passkeys::{self, InsertOutcome, PasskeyInput};
use crate::passwords::{presets, tags};

/// The parts of the model this module writes.
pub struct Extras {
    pub tag_colors: Vec<(String, String)>,
    pub passkeys: Vec<PasskeyInput>,
    pub presets: Vec<PresetInput>,
}

/// The rows created (for the ledger) and the places for the report.
#[derive(Default)]
pub struct ExtrasOutcome {
    pub tag_colors: Vec<(String, Option<String>)>,
    pub passkeys: Vec<String>,
    pub presets: Vec<String>,
    pub problems: Vec<Problem>,
}

pub fn write(tx: &mut CrdtTransaction<'_>, extras: &Extras) -> Result<ExtrasOutcome> {
    let mut outcome = ExtrasOutcome {
        tag_colors: write_tag_colors(tx, &extras.tag_colors)?,
        ..ExtrasOutcome::default()
    };
    for passkey in &extras.passkeys {
        let mut input = passkey.clone();
        input.item_id = None;
        match passkeys::insert(tx, &input)? {
            InsertOutcome::Created(id) => outcome.passkeys.push(id),
            InsertOutcome::Duplicate => outcome.problems.push(Problem::field(
                AttentionKind::PasskeyDuplicate,
                &format!("passkey {}", passkey.relying_party_id),
            )),
        }
    }
    write_presets(tx, &extras.presets, &mut outcome)?;
    Ok(outcome)
}

/// A tag of the vault takes the colour of the source only when it has none of its own; tags that
/// no imported entry carries are not created for a colour.
fn write_tag_colors(
    tx: &mut CrdtTransaction<'_>,
    colors: &[(String, String)],
) -> Result<Vec<(String, Option<String>)>> {
    if colors.is_empty() {
        return Ok(Vec::new());
    }
    let existing: Vec<(String, String, Option<String>)> = tx.query_map(
        "SELECT id, name, color FROM haex_passwords_tags",
        params![],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let mut changed = Vec::new();
    for (name, color) in colors {
        let wanted = fold(name);
        let tag = existing
            .iter()
            .find(|(_, stored, current)| current.is_none() && fold(stored) == wanted);
        if let Some((id, _, current)) = tag {
            tags::set_color(tx, id, Some(color))?;
            changed.push((id.clone(), current.clone()));
        }
    }
    Ok(changed)
}

/// A preset whose name the vault already has is skipped; an imported default only becomes the
/// default when the vault has none, so the user's own choice stays.
fn write_presets(
    tx: &mut CrdtTransaction<'_>,
    imported: &[PresetInput],
    outcome: &mut ExtrasOutcome,
) -> Result<()> {
    if imported.is_empty() {
        return Ok(());
    }
    let existing = presets::list(tx)?;
    let mut names: Vec<String> = existing
        .iter()
        .map(|p| p.name.trim().to_lowercase())
        .collect();
    let mut has_default = existing.iter().any(|p| p.is_default);
    for preset in imported {
        let name = preset.name.trim().to_lowercase();
        if names.contains(&name) {
            continue;
        }
        let input = PresetInput {
            id: String::new(),
            is_default: preset.is_default && !has_default,
            ..preset.clone()
        };
        match presets::save(tx, &input) {
            Ok(id) => {
                has_default |= input.is_default;
                names.push(name);
                outcome.presets.push(id);
            }
            Err(HolziError::InvalidInput { .. }) => outcome.problems.push(Problem::field(
                AttentionKind::ValueNotStorable,
                &format!("generator preset {}", preset.name),
            )),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Removes the passkeys and presets a run created (its rollback).
pub fn undo(
    tx: &mut CrdtTransaction<'_>,
    tag_colors: &[(String, Option<String>)],
    passkeys: &[String],
    presets: &[String],
) -> Result<()> {
    for (id, color) in tag_colors {
        tx.execute(
            "UPDATE haex_passwords_tags SET color = ?1 WHERE id = ?2",
            params![color, id],
        )?;
    }
    for id in passkeys {
        tx.execute(
            "DELETE FROM haex_passwords_passkeys WHERE id = ?1",
            params![id],
        )?;
    }
    for id in presets {
        tx.execute(
            "DELETE FROM haex_passwords_generator_presets WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "apply_extras_tests.rs"]
mod tests;

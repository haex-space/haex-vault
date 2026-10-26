//! Reading a share in (spec 060 FR-024 to FR-025a): everything is recomputed from the text, the
//! person's choices are checked against what each item allows, and then everything is written in the
//! caller's one transaction or nothing is. A share only ever adds: it fills an empty field, adds a
//! value, adds holzi addresses next to the ones a contact has. It replaces only what the person
//! chose for exactly that item, and never the confirmed vault identity of a contact.

use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::model::{
    ContactShareChoice, ContactShareDecision, ContactShareIdentity, ContactShareItem,
    ContactShareState, ContactShareTarget,
};
use super::preview::{plan, Payload, Plan};
use crate::contacts::card::received;
use crate::contacts::model::{Contact, Head, Origin};
use crate::contacts::store;
use crate::error::{HolziError, Result};
use crate::passwords::clock;

fn invalid(reason: &str) -> HolziError {
    HolziError::ShareInvalid {
        reason: reason.to_owned(),
    }
}

/// The choice that counts for an item: the decision when there is one and the item allows it, else
/// the default. A decision the item does not allow refuses the whole share.
fn chosen(
    item: &ContactShareItem,
    decision: Option<&ContactShareDecision>,
) -> Result<Option<(ContactShareChoice, Option<String>)>> {
    let Some(decision) = decision else {
        return Ok(item.default.map(|choice| (choice, None)));
    };
    if !item.choices.contains(&decision.choice) {
        return Err(invalid("a choice the item does not offer"));
    }
    if decision.choice == ContactShareChoice::Update && item.existing.is_empty() {
        // A single field takes `Update` without a value to name.
        if decision.value_id.is_some() {
            return Err(invalid("a field has no value to replace"));
        }
    } else if decision.choice == ContactShareChoice::Update {
        let named = decision
            .value_id
            .as_deref()
            .ok_or_else(|| invalid("an update names the value to replace"))?;
        if !item.existing.iter().any(|existing| existing.id == named) {
            return Err(invalid("the value is not one of the target's"));
        }
    } else if decision.value_id.is_some() {
        return Err(invalid("only an update names a value"));
    }
    Ok(Some((decision.choice, decision.value_id.clone())))
}

fn decisions_by_item<'a>(
    plan: &Plan,
    decisions: &'a [ContactShareDecision],
) -> Result<Vec<Option<&'a ContactShareDecision>>> {
    let mut by_item = vec![None; plan.preview.items.len()];
    for decision in decisions {
        let slot = usize::try_from(decision.item)
            .ok()
            .and_then(|index| by_item.get_mut(index))
            .ok_or_else(|| invalid("a decision names no item"))?;
        if slot.is_some() {
            return Err(invalid("two decisions for one item"));
        }
        *slot = Some(decision);
    }
    Ok(by_item)
}

/// Reads the share in as a new contact or into an existing one, as chosen. `preview_revision` is
/// the revision of the preview the person saw; an existing target must still be what it was
/// ([`HolziError::SharePreviewStale`]). Returns the contact as it is stored afterwards.
pub fn apply(
    tx: &mut CrdtTransaction<'_>,
    text: &str,
    target: Option<&ContactShareTarget>,
    decisions: &[ContactShareDecision],
    preview_revision: Option<&str>,
    origin: &Origin,
) -> Result<Contact> {
    let plan = plan(tx, text, target)?;
    if plan.target.is_some() && preview_revision != plan.preview.target_revision.as_deref() {
        return Err(HolziError::SharePreviewStale);
    }
    if plan.preview.blocked {
        return Err(invalid("the vault is blocked"));
    }
    if matches!(plan.preview.identity, ContactShareIdentity::Mismatch { .. }) {
        return Err(invalid(
            "the contact has another vault identity; only a new contact can take this share",
        ));
    }
    let by_item = decisions_by_item(&plan, decisions)?;
    let mut picks = Vec::with_capacity(plan.preview.items.len());
    for (item, decision) in plan.preview.items.iter().zip(&by_item) {
        picks.push(chosen(item, *decision)?);
    }

    let now = clock::now();
    let contact_id = match &plan.target {
        Some(id) => id.clone(),
        None => {
            let id = Uuid::new_v4().to_string();
            let mut head = Head::default();
            for payload in &plan.payloads {
                if let Payload::Head { column, text } = payload {
                    set_in(&mut head, column, text.clone());
                }
            }
            if head.display_name.is_none() {
                head.display_name = plan.new_display_name.clone();
            }
            store::insert(tx, &id, &head, origin, &now)?;
            id
        }
    };

    for ((payload, item), pick) in plan.payloads.iter().zip(&plan.preview.items).zip(&picks) {
        let existing_target = plan.target.is_some();
        match payload {
            Payload::Head { column, text } => {
                // A new contact got its fields above.
                let write = existing_target
                    && match item.state {
                        ContactShareState::New => true,
                        ContactShareState::Conflict => {
                            matches!(pick, Some((ContactShareChoice::Update, _)))
                        }
                        ContactShareState::Same => false,
                    };
                if write {
                    store::set_head(tx, &contact_id, column, Some(text))?;
                }
            }
            Payload::Value { kind, label, value } => match pick {
                Some((ContactShareChoice::Add, _)) => {
                    let position = store::next_position(tx, &contact_id, kind)?;
                    store::insert_value(tx, &contact_id, kind, label, value, position)?;
                }
                Some((ContactShareChoice::Update, Some(value_id))) => {
                    // Checked against the preview's list of the target's values already; the
                    // owner is checked once more at the table.
                    match store::value_owner(tx, value_id)? {
                        Some((owner, held_kind)) if owner == contact_id && &held_kind == kind => {
                            store::update_value(tx, value_id, label, value, None)?;
                        }
                        _ => return Err(invalid("the value is not one of the target's")),
                    }
                }
                _ => {}
            },
            Payload::Holzi => {
                let card = plan
                    .decoded
                    .card
                    .as_ref()
                    .ok_or_else(|| invalid("the share has no card"))?;
                match pick {
                    Some((ContactShareChoice::Add, _)) => {
                        received::save(tx, &contact_id, card)?;
                    }
                    Some((ContactShareChoice::Replace, _)) => {
                        let older = received::stored(tx, &card.card.vault)?
                            .is_some_and(|(_, old)| card.card.issued_at <= old.card.issued_at);
                        if older {
                            return Err(HolziError::CardOlder);
                        }
                        received::replace(tx, &contact_id, card)?;
                    }
                    _ => {}
                }
            }
        }
    }
    store::touch(tx, &contact_id, &now)?;
    store::load(tx, &contact_id)
}

fn set_in(head: &mut Head, column: &str, text: String) {
    let field = match column {
        "display_name" => &mut head.display_name,
        "given_name" => &mut head.given_name,
        "family_name" => &mut head.family_name,
        "middle_name" => &mut head.middle_name,
        "name_prefix" => &mut head.name_prefix,
        "name_suffix" => &mut head.name_suffix,
        "nickname" => &mut head.nickname,
        "organization" => &mut head.organization,
        "job_title" => &mut head.job_title,
        "note" => &mut head.note,
        _ => return,
    };
    *field = Some(text);
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;

//! Passkeys shown at another entry by a link (spec 036, FR-046, research R6, data-model.md):
//! `haex_passwords_passkey_links` joins a target entry to a passkey of another entry. The key stays
//! with the passkey; a link is never a copy. The id is derived from the pair, so linking twice or on
//! two devices gives one row. Links go before the passkey or the entry they hang on (the sync applies
//! remote deletes without foreign keys).

use std::collections::HashMap;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::ids::passkey_link_id;
use super::model_passkeys::{LinkedFrom, PasskeyView};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

/// The entry a passkey belongs to; `Ok(None)` for a passkey without an entry, `NotFound` for none.
fn owner(q: &mut impl Query, passkey_id: &str) -> Result<Option<String>> {
    q.query_row(
        "SELECT item_id FROM haex_passwords_passkeys WHERE id = ?1",
        params![passkey_id],
        |r| r.get::<_, Option<String>>(0),
    )?
    .ok_or(HolziError::PasswordsNotFound)
}

/// Shows the passkey at `item_id`. Refused (`InvalidInput`, `passkeyId`) for a passkey of that
/// entry itself and for one without an entry (a link always leads to a passkey with an entry, so
/// there are no links on links). Returns the link's id; an existing link stays as it is.
pub fn link(tx: &mut CrdtTransaction<'_>, item_id: &str, passkey_id: &str) -> Result<String> {
    // Only a login shows a passkey; a pass is no way in (spec 092, FR-024).
    let exists = tx
        .query_row(
            "SELECT COUNT(*) FROM haex_passwords_item_details WHERE id = ?1 AND kind = 'login'",
            params![item_id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0);
    if exists == 0 {
        return Err(HolziError::PasswordsNotFound);
    }
    match owner(tx, passkey_id)? {
        Some(owner) if owner != item_id => {}
        _ => {
            return Err(HolziError::InvalidInput {
                reason: "passkeyId".into(),
            })
        }
    }
    let id = passkey_link_id(item_id, passkey_id).to_string();
    let known = tx
        .query_row(
            "SELECT COUNT(*) FROM haex_passwords_passkey_links WHERE id = ?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0);
    if known == 0 {
        tx.execute(
            "INSERT INTO haex_passwords_passkey_links (id, item_id, passkey_id) VALUES (?1, ?2, ?3)",
            params![id, item_id, passkey_id],
        )?;
    }
    Ok(id)
}

/// Drops the link of `passkey_id` at `item_id` ("Verweis lösen"); the passkey stays.
pub fn unlink(tx: &mut CrdtTransaction<'_>, item_id: &str, passkey_id: &str) -> Result<()> {
    let changed = tx.execute(
        "DELETE FROM haex_passwords_passkey_links WHERE item_id = ?1 AND passkey_id = ?2",
        params![item_id, passkey_id],
    )?;
    if changed == 0 {
        return Err(HolziError::PasswordsNotFound);
    }
    Ok(())
}

/// Removes every link to a passkey, before the passkey goes.
pub fn delete_for_passkey(tx: &mut CrdtTransaction<'_>, passkey_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_passwords_passkey_links WHERE passkey_id = ?1",
        params![passkey_id],
    )?;
    Ok(())
}

/// Removes the links of an entry as target and those to its own passkeys, before it goes.
pub fn delete_for_item(tx: &mut CrdtTransaction<'_>, item_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_passwords_passkey_links WHERE item_id = ?1 \
         OR passkey_id IN (SELECT id FROM haex_passwords_passkeys WHERE item_id = ?1)",
        params![item_id],
    )?;
    Ok(())
}

/// The passkeys of other entries shown at `item_id`, each with the entry it belongs to.
pub fn linked_views(q: &mut impl Query, item_id: &str) -> Result<Vec<PasskeyView>> {
    Ok(q.query_map(
        "SELECT p.id, p.relying_party_id, p.relying_party_name, p.user_name, p.nickname, \
                p.algorithm, p.created_at, p.last_used_at, p.item_id, p.is_discoverable, \
                p.sign_count, d.title \
         FROM haex_passwords_passkey_links l \
         JOIN haex_passwords_passkeys p ON p.id = l.passkey_id \
         JOIN haex_passwords_item_details d ON d.id = p.item_id \
         WHERE l.item_id = ?1 ORDER BY l.rowid",
        params![item_id],
        |r| {
            let owner: String = r.get(8)?;
            Ok(PasskeyView {
                id: r.get(0)?,
                relying_party_id: r.get(1)?,
                relying_party_name: r.get(2)?,
                user_name: r.get(3)?,
                nickname: r.get(4)?,
                algorithm: r.get(5)?,
                created_at: r.get(6)?,
                last_used_at: r.get(7)?,
                item_id: Some(owner.clone()),
                is_discoverable: r.get::<_, i64>(9)? != 0,
                sign_count: r.get(10)?,
                linked_from: Some(LinkedFrom {
                    item_id: owner,
                    title: r.get(11)?,
                }),
            })
        },
    )?)
}

/// The passkey ids `item_id` shows: its own and the linked ones, in that order. A link whose
/// passkey is gone (a delete that came by sync before the link's) is left out.
pub fn shown_passkey_ids(q: &mut impl Query, item_id: &str) -> Result<Vec<String>> {
    Ok(q.query_map(
        "SELECT id FROM (\
           SELECT id, 0 AS linked, rowid AS pos FROM haex_passwords_passkeys WHERE item_id = ?1 \
           UNION ALL \
           SELECT l.passkey_id, 1, l.rowid FROM haex_passwords_passkey_links l \
           JOIN haex_passwords_passkeys p ON p.id = l.passkey_id \
           WHERE l.item_id = ?1 AND p.item_id IS NOT NULL AND p.item_id <> ?1) \
         ORDER BY linked, pos",
        params![item_id],
        |r| r.get(0),
    )?)
}

/// For each of `sources`, how many links to its passkeys other entries hold (they drop when the
/// entry is deleted for good).
pub fn links_to_items(q: &mut impl Query, sources: &[String]) -> Result<HashMap<String, u32>> {
    let mut counts = HashMap::new();
    for source in sources {
        let count = q
            .query_row(
                "SELECT COUNT(*) FROM haex_passwords_passkey_links l \
                 JOIN haex_passwords_passkeys p ON p.id = l.passkey_id \
                 WHERE p.item_id = ?1 AND l.item_id <> ?1",
                params![source],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0);
        counts.insert(source.clone(), u32::try_from(count).unwrap_or(u32::MAX));
    }
    Ok(counts)
}

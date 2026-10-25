//! What the serving device does when shares change (spec 056 FR-022–FR-026, research R10, R11): it
//! follows the vault's changes and, at the start and after every change of shares or contacts,
//! - confirms the changes to the shares it serves (`served_at`), so other own devices stop showing
//!   "Änderung wartet auf ‹Gerät›";
//! - writes what it finds at their roots (`source_state`);
//! - forgets grants and received shares of vaults whose card it saw and that is gone now;
//! - cancels the running requests a revoke or a downgrade no longer covers.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::broadcast::error::RecvError;

use crate::contacts::service::{Caller as ContactsCaller, ContactsService, VaultCard};
use crate::files::peer::connect::key_of;
use crate::files::peer::handshake;
use crate::files::peer::serve::roots;
use crate::files::peer::serve::ServeContext;
use crate::files::share::store::{self, FileShareRow};
use crate::files::share::{received, FileShareSourceState, FEATURE};
use crate::vault_gate::VaultDb;

/// Whether a change of `table` can change what this device serves.
fn concerns_shares(table: &str) -> bool {
    table.starts_with("haex_file_share") || table.starts_with("haex_contacts")
}

/// Follows the vault's changes for `ctx`; never ends, the session drops it. Without a files server
/// there is nothing to follow.
pub async fn follow(ctx: Option<Arc<ServeContext>>) {
    let Some(ctx) = ctx else {
        return std::future::pending().await;
    };
    let mut changes = ctx.changes.subscribe();
    let mut cards = HashSet::new();
    apply(&ctx, &mut cards).await;
    loop {
        match changes.recv().await {
            Ok(tables) if !tables.iter().any(|table| concerns_shares(table)) => continue,
            // Fell behind: anything may have changed.
            Ok(_) | Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => return std::future::pending().await,
        }
        // One pass covers what came in meanwhile, its own writes included.
        while changes.try_recv().is_ok() {}
        apply(&ctx, &mut cards).await;
    }
}

/// One pass over the shares this device serves; `cards` are the vaults earlier passes saw with a
/// card. A failure is logged and the next change tries again.
pub async fn apply(ctx: &ServeContext, cards: &mut HashSet<String>) {
    if let Err(error) = forget_lost_contacts(ctx, cards).await {
        log::warn!("files: grants of removed contacts were not cleaned up: {error}");
    }
    if let Err(error) = confirm(ctx).await {
        log::warn!("files: changes to shares were not confirmed: {error}");
    }
    cancel_revoked(ctx).await;
}

/// What is at the root of `share` now (FR-022): there, gone, or not reachable.
pub fn found_at(share: &FileShareRow) -> FileShareSourceState {
    let Ok(at) = roots::on_device(&share.path) else {
        return FileShareSourceState::Missing;
    };
    match std::fs::symlink_metadata(at.path()) {
        Ok(_) => FileShareSourceState::Ok,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => FileShareSourceState::Missing,
        Err(_) => FileShareSourceState::Unavailable,
    }
}

/// After a contact's request reached the root of `share` or not: writes what is found there when it
/// differs from what is stored.
pub async fn note_source(db: &VaultDb, share: &FileShareRow, reached: bool) {
    let found = if reached {
        FileShareSourceState::Ok
    } else {
        let row = share.clone();
        match tokio::task::spawn_blocking(move || found_at(&row)).await {
            Ok(found) => found,
            Err(_) => return,
        }
    };
    if found == share.source_state {
        return;
    }
    let id = share.id.clone();
    if let Err(error) = db
        .write(move |tx| Ok(store::set_source_state(tx, &id, found)?))
        .await
    {
        log::warn!("files: the state of a share's source was not written: {error}");
    }
}

/// Confirms every change to the shares served here and writes what is at their roots.
async fn confirm(ctx: &ServeContext) -> crate::error::Result<()> {
    let device = ctx.device.clone();
    let shares = ctx
        .db
        .read(move |q| Ok(store::shares_served_by(q, &device)?))
        .await?;
    let found = tokio::task::spawn_blocking(move || {
        shares
            .into_iter()
            .map(|share| {
                let found = found_at(&share);
                (share, found)
            })
            .filter(|(share, found)| share.is_pending() || *found != share.source_state)
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    if found.is_empty() {
        return Ok(());
    }
    ctx.db
        .write(move |tx| {
            for (share, state) in &found {
                if share.is_pending() {
                    store::set_served(tx, &share.id, &share.changed_at)?;
                }
                store::set_source_state(tx, &share.id, *state)?;
            }
            Ok(())
        })
        .await
}

/// Deletes the grants in shares served here and the received shares of vaults whose card an earlier
/// pass saw and that is gone now (FR-023). Only then: a grant that sync brings before its card is
/// no orphan. A vault whose card went while this device was off keeps its rows, which every request
/// refuses anyway (research R11). A blocked vault keeps them too, and unblocking brings it back.
async fn forget_lost_contacts(
    ctx: &ServeContext,
    cards: &mut HashSet<String>,
) -> crate::error::Result<()> {
    let device = ctx.device.clone();
    let (grants, owners) = ctx
        .db
        .read(move |q| {
            let mut grants = Vec::new();
            for share in store::shares_served_by(q, &device)? {
                for grant in store::grants_of(q, &share.id)? {
                    grants.push((share.id.clone(), grant.vault_pubkey));
                }
            }
            Ok((grants, received::owners(q)?))
        })
        .await?;
    let service = ContactsService::new(ctx.db.clone());
    let caller = ContactsCaller::Internal { feature: FEATURE };
    let mut gone = Vec::new();
    let mut vaults: Vec<&String> = grants.iter().map(|(_, vault)| vault).collect();
    vaults.extend(&owners);
    vaults.sort();
    vaults.dedup();
    for vault in vaults {
        let Some(key) = key_of(vault) else { continue };
        match service.card_of_vault(&caller, key).await? {
            VaultCard::Unknown if cards.remove(vault) => gone.push(vault.clone()),
            VaultCard::Unknown => {}
            VaultCard::Stored { .. } | VaultCard::Blocked => {
                cards.insert(vault.clone());
            }
        }
    }
    if gone.is_empty() {
        return Ok(());
    }
    let now = crate::passwords::clock::now();
    ctx.db
        .write(move |tx| {
            for (share, vault) in grants.iter().filter(|(_, vault)| gone.contains(vault)) {
                store::delete_grant(tx, share, vault, &now)?;
            }
            for vault in owners.iter().filter(|vault| gone.contains(vault)) {
                received::forget_vault(tx, vault)?;
            }
            Ok(())
        })
        .await
}

/// Cancels the running requests whose grant is gone or lower now, or whose vault is no contact any
/// more (FR-024).
async fn cancel_revoked(ctx: &ServeContext) {
    for key in ctx.streams.keys() {
        let (share, vault) = (key.share.clone(), crate::sync::keys::hex(&key.vault));
        let level = ctx
            .db
            .read(move |q| Ok(store::grant_for(q, &share, &vault)?))
            .await;
        let covered = matches!(level, Ok(Some(level)) if level >= key.level)
            && handshake::still_contact(&ctx.db, key.vault).await;
        if !covered {
            ctx.streams.cancel(&key);
        }
    }
}

#[cfg(test)]
#[path = "served_tests.rs"]
mod tests;

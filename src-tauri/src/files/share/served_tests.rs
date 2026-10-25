//! The serving device after changes (spec 056 US4, T061, T062): a change made elsewhere stays
//! pending until this device applied it, a missing root shows as missing and back as ok, and the
//! grants and received shares of a vault whose card went are forgotten, but not before it came.

// A card is removed directly, as deleting its contact does by cascade.
#![allow(clippy::disallowed_methods)]

use std::collections::HashSet;

use haex_crdt::rusqlite::params;

use super::apply;
use crate::files::peer::test_support::{store_card, Device};
use crate::files::peer::wire::WireShare;
use crate::files::share::received::{self, Answerer};
use crate::files::share::store;
use crate::files::share::{FileShareItemKind, FileShareLevel, FileShareSourceState};
use crate::sync::keys::hex;
use crate::sync::test_support::Member;

/// The share `id` as stored on `device`.
fn row(device: &Device, id: &str) -> store::FileShareRow {
    let id = id.to_owned();
    device
        .db
        .read_blocking(move |q| Ok(store::get_share(q, &id)?))
        .expect("read")
        .expect("the share")
}

/// Removes the card of `vault` on `device`.
fn forget_card(device: &Device, vault: [u8; 32]) {
    device
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "DELETE FROM haex_contacts_cards WHERE vault_pubkey = ?1",
                params![hex(&vault)],
            )?;
            Ok(())
        })
        .expect("forget the card");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_change_stays_pending_until_the_serving_device_applied_it() {
    let anna = Device::of(Member::genesis()).await;
    anna.write("Fotos/a.jpg", b"a");
    // Written as another device of the vault would: `served_at` stays empty.
    let share = anna.share(
        &anna.root().join("Fotos"),
        FileShareItemKind::Folder,
        "Fotos",
    );
    assert!(row(&anna, &share).is_pending(), "FR-026");

    apply(&anna.ctx, &mut HashSet::new()).await;
    let applied = row(&anna, &share);
    assert!(!applied.is_pending());
    assert_eq!(
        applied.served_at.as_deref(),
        Some(applied.changed_at.as_str())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_missing_root_shows_as_missing_and_back_as_ok() {
    let anna = Device::of(Member::genesis()).await;
    anna.write("Fotos/a.jpg", b"a");
    let share = anna.share(
        &anna.root().join("Fotos"),
        FileShareItemKind::Folder,
        "Fotos",
    );
    std::fs::rename(anna.root().join("Fotos"), anna.root().join("Bilder")).expect("move away");
    apply(&anna.ctx, &mut HashSet::new()).await;
    let missing = row(&anna, &share);
    assert_eq!(
        missing.source_state,
        FileShareSourceState::Missing,
        "FR-022"
    );
    assert!(
        missing.path.ends_with("Fotos"),
        "never moved to another path"
    );

    std::fs::rename(anna.root().join("Bilder"), anna.root().join("Fotos")).expect("back");
    apply(&anna.ctx, &mut HashSet::new()).await;
    assert_eq!(row(&anna, &share).source_state, FileShareSourceState::Ok);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_grant_that_arrives_before_its_card_stays() {
    let anna = Device::of(Member::genesis()).await;
    let ben = Member::genesis();
    let share = anna.share(&anna.root(), FileShareItemKind::Folder, "Alles");
    anna.grant(&share, ben.vault, FileShareLevel::Read);
    apply(&anna.ctx, &mut HashSet::new()).await;
    let id = share.clone();
    let grants = anna
        .db
        .read_blocking(move |q| Ok(store::grants_of(q, &id)?))
        .expect("grants");
    assert_eq!(grants.len(), 1, "no card seen yet: no orphan");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_vault_without_a_card_loses_its_grants_and_its_shares() {
    let anna = Device::of(Member::genesis()).await;
    let ben = Device::of(Member::genesis()).await;
    let cleo = Device::of(Member::genesis()).await;
    store_card(&ben, &anna).await;
    store_card(&cleo, &anna).await;
    let share = anna.share(&anna.root(), FileShareItemKind::Folder, "Alles");
    anna.grant(&share, ben.member.vault, FileShareLevel::Read);
    anna.grant(&share, cleo.member.vault, FileShareLevel::Read);
    // Anna also holds a share Ben gave her.
    let (ben_vault, device) = (hex(&ben.member.vault), hex(&[7; 32]));
    anna.db
        .write_blocking(move |tx| {
            let from = Answerer {
                vault: &ben_vault,
                device: &device,
                endpoint: &device,
            };
            let shares = [WireShare {
                id: "s1".to_owned(),
                name: "Urlaub".to_owned(),
                item_kind: FileShareItemKind::Folder,
                level: FileShareLevel::Read,
            }];
            Ok(received::apply_answer(tx, from, &shares, "t")?)
        })
        .expect("received");

    let mut cards = HashSet::new();
    apply(&anna.ctx, &mut cards).await;
    forget_card(&anna, ben.member.vault);
    apply(&anna.ctx, &mut cards).await;

    let id = share.clone();
    let grants = anna
        .db
        .read_blocking(move |q| Ok(store::grants_of(q, &id)?))
        .expect("grants");
    let vaults: Vec<String> = grants.into_iter().map(|grant| grant.vault_pubkey).collect();
    assert_eq!(vaults, [hex(&cleo.member.vault)], "FR-023");
    let ben_vault = hex(&ben.member.vault);
    let kept = anna
        .db
        .read_blocking(move |q| Ok(received::of_vault(q, &ben_vault)?))
        .expect("received");
    assert!(kept.is_empty());
}

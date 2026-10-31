//! Spec 069, the store of the marketplaces against a real vault (T012): new rows go to the end,
//! the order can be rewritten, every row but the preset can be deleted, and the preset is added
//! once.

#[path = "common/marketplace_fixture.rs"]
mod marketplace_fixture;

use holzi_lib::marketplace::model::{MarketplaceAuthType, MarketplaceInput};
use holzi_lib::marketplace::store::{self, MarketplaceRow, PRESET_ID};
use marketplace_fixture::{vault, TestVault};

fn input(name: &str, url: &str) -> MarketplaceInput {
    MarketplaceInput {
        id: None,
        name: name.to_owned(),
        base_url: url.to_owned(),
        auth_type: MarketplaceAuthType::None,
        credentials_item_id: None,
        new_credentials: None,
    }
}

fn rows(vault: &TestVault) -> Vec<MarketplaceRow> {
    vault
        .db
        .read_blocking(|q| Ok(store::list(q)?))
        .expect("list")
}

fn ids(vault: &TestVault) -> Vec<String> {
    rows(vault).into_iter().map(|row| row.id).collect()
}

fn add(vault: &TestVault, name: &str, url: &str) -> String {
    let input = input(name, url);
    vault
        .db
        .write_blocking(|tx| Ok(store::save(tx, &input, 10)?))
        .expect("save")
}

#[test]
fn a_new_vault_has_no_marketplace_until_one_is_added() {
    assert!(rows(&vault()).is_empty());
}

#[test]
fn new_marketplaces_go_to_the_end_and_keep_their_normalized_address() {
    let vault = vault();
    let first = add(&vault, "  Team ", "https://example.org/market/?q=1#top");
    let second = add(&vault, "Home", "http://192.168.1.5:8080");
    let all = rows(&vault);
    assert_eq!(ids(&vault), [first.clone(), second.clone()]);
    assert_eq!(all[0].name, "Team");
    assert_eq!(all[0].base_url, "https://example.org/market");
    assert_eq!(all[1].base_url, "http://192.168.1.5:8080");
    assert!(all.iter().all(|row| row.enabled));
    assert!(all[0].sort_order < all[1].sort_order);
}

#[test]
fn a_marketplace_can_be_changed_switched_off_and_reordered() {
    let vault = vault();
    let a = add(&vault, "A", "https://a.example");
    let b = add(&vault, "B", "https://b.example");

    let mut change = input("A2", "https://a2.example/api");
    change.id = Some(a.clone());
    vault
        .db
        .write_blocking(|tx| Ok(store::save(tx, &change, 20)?))
        .expect("change");
    vault
        .db
        .write_blocking(|tx| Ok(store::set_enabled(tx, &a, false, 21)?))
        .expect("switch off");
    vault
        .db
        .write_blocking(|tx| Ok(store::reorder(tx, &[b.clone(), a.clone()], 22)?))
        .expect("reorder");

    let all = rows(&vault);
    assert_eq!(ids(&vault), [b, a]);
    assert_eq!(all[1].name, "A2");
    assert_eq!(all[1].base_url, "https://a2.example/api");
    assert!(!all[1].enabled);
    assert!(all[0].enabled);
}

#[test]
fn a_row_not_named_in_a_reorder_keeps_its_place_behind_the_named_ones() {
    let vault = vault();
    let a = add(&vault, "A", "https://a.example");
    let b = add(&vault, "B", "https://b.example");
    let late = add(&vault, "Late", "https://late.example");
    vault
        .db
        .write_blocking(|tx| Ok(store::reorder(tx, &[b.clone(), a.clone()], 30)?))
        .expect("reorder");
    assert_eq!(ids(&vault), [b, a, late]);
}

#[test]
fn every_row_can_be_deleted_and_an_unknown_one_is_refused() {
    let vault = vault();
    let a = add(&vault, "A", "https://a.example");
    vault
        .db
        .write_blocking(|tx| Ok(store::delete(tx, &a)?))
        .expect("delete");
    assert!(rows(&vault).is_empty());
    let again = vault.db.write_blocking(|tx| Ok(store::delete(tx, &a)?));
    assert!(again.is_err(), "the row is gone");
}

#[test]
fn login_needs_an_entry_and_an_https_address() {
    let vault = vault();
    let mut bearer = input("Private", "https://private.example");
    bearer.auth_type = MarketplaceAuthType::Bearer;
    let without_entry = vault
        .db
        .write_blocking(|tx| Ok(store::save(tx, &bearer, 1)?));
    assert!(without_entry.is_err(), "bearer without an entry");

    bearer.credentials_item_id = Some("item-1".to_owned());
    bearer.base_url = "http://private.example".to_owned();
    let over_http = vault
        .db
        .write_blocking(|tx| Ok(store::save(tx, &bearer, 1)?));
    assert!(over_http.is_err(), "a login over http");

    bearer.base_url = "https://private.example".to_owned();
    let id = vault
        .db
        .write_blocking(|tx| Ok(store::save(tx, &bearer, 1)?))
        .expect("save");
    let row = vault
        .db
        .read_blocking(|q| Ok(store::get(q, &id)?))
        .expect("get")
        .expect("row");
    assert_eq!(row.auth_type, MarketplaceAuthType::Bearer);
    assert_eq!(row.credentials_item_id.as_deref(), Some("item-1"));
}

#[test]
fn a_marketplace_without_login_forgets_a_stale_entry_id() {
    let vault = vault();
    let mut plain = input("Open", "https://open.example");
    plain.credentials_item_id = Some("stale".to_owned());
    let id = vault
        .db
        .write_blocking(|tx| Ok(store::save(tx, &plain, 1)?))
        .expect("save");
    let row = vault
        .db
        .read_blocking(|q| Ok(store::get(q, &id)?))
        .expect("get")
        .expect("row");
    assert_eq!(row.credentials_item_id, None);
}

#[test]
fn the_preset_is_added_once_and_a_second_call_changes_nothing() {
    let vault = vault();
    let first = vault
        .db
        .write_blocking(|tx| Ok(store::add_preset(tx, 1)?))
        .expect("add preset");
    assert_eq!(first.id, PRESET_ID);
    assert_eq!(first.name, store::PRESET_NAME);
    assert_eq!(first.base_url, store::PRESET_URL);

    vault
        .db
        .write_blocking(|tx| Ok(store::set_enabled(tx, PRESET_ID, false, 2)?))
        .expect("switch off");
    let second = vault
        .db
        .write_blocking(|tx| Ok(store::add_preset(tx, 3)?))
        .expect("add again");
    assert!(!second.enabled, "a switch the user turned off stays off");
    assert_eq!(rows(&vault).len(), 1, "no duplicate");
}

#[test]
fn the_preset_is_never_deleted() {
    let vault = vault();
    vault
        .db
        .write_blocking(|tx| Ok(store::add_preset(tx, 1)?))
        .expect("add preset");
    let deleted = vault
        .db
        .write_blocking(|tx| Ok(store::delete(tx, PRESET_ID)?));
    assert!(deleted.is_err(), "the preset is only switched off");
    assert_eq!(ids(&vault), [PRESET_ID]);
}

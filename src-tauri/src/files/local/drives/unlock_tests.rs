//! A drive's entry in the password manager (spec 064 T049, T056): found by its exact address, read
//! for the unlock, created from a typed password and linked to a picked entry.

use super::*;
use crate::passwords::model::{Target, TargetKind};
use crate::passwords::test_support::MARKER;
use crate::remote_storage::test_support::vault;

const KEY: &str = "3F2A-Stick";

async fn entry(passwords: &PasswordsService, title: &str, url: Option<&str>) -> String {
    passwords
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some(title.to_owned()),
                username: Some("anna".to_owned()),
                password: Some(MARKER.to_owned()),
                url: url.map(str::to_owned),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("create")
}

async fn header(passwords: &PasswordsService, item_id: &str) -> ItemHeader {
    passwords
        .get_item(&Caller::User, item_id.to_owned())
        .await
        .expect("the entry")
        .header
}

fn tags(header: &ItemHeader) -> Vec<&str> {
    header.tags.iter().map(|tag| tag.name.as_str()).collect()
}

#[test]
fn the_address_holds_the_key_in_lower_case() {
    assert_eq!(entry_url(KEY), "haex:drive:3f2a-stick");
}

#[tokio::test]
async fn only_entries_with_exactly_the_drive_s_address_match() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    let url = entry_url(KEY);
    assert!(matching_entries(&passwords, KEY).await.unwrap().is_empty());

    let first = entry(&passwords, "Stick", Some(&url)).await;
    entry(&passwords, "Großgeschrieben", Some("haex:drive:3F2A-STICK")).await;
    entry(&passwords, "Länger", Some(&format!("{url}-alt"))).await;
    entry(&passwords, "Ohne", None).await;
    let found = matching_entries(&passwords, KEY).await.unwrap();
    assert_eq!(
        found.iter().map(|e| e.item_id.as_str()).collect::<Vec<_>>(),
        [first.as_str()]
    );

    let second = entry(&passwords, "Stick 2", Some(&url)).await;
    let found = matching_entries(&passwords, KEY).await.unwrap();
    assert_eq!(found.len(), 2);
    assert!(!format!("{found:?}").contains(MARKER), "no password listed");

    passwords
        .trash_targets(
            &Caller::User,
            vec![Target {
                kind: TargetKind::Item,
                id: second,
            }],
        )
        .await
        .unwrap();
    assert_eq!(matching_entries(&passwords, KEY).await.unwrap().len(), 1);
}

#[tokio::test]
async fn the_password_of_an_entry_is_read_for_the_unlock() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    let id = entry(&passwords, "Stick", Some(&entry_url(KEY))).await;
    assert_eq!(
        read_password(&passwords, &id).await.unwrap().expose(),
        MARKER
    );
}

#[tokio::test]
async fn the_picker_lists_entries_with_a_password_drive_entries_first() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    entry(&passwords, "Bank", Some("https://bank.example")).await;
    let typed = DriveSecret::new(MARKER.to_owned());
    remember_typed(&passwords, KEY, "Laufwerk: STICK", &typed)
        .await
        .unwrap();
    passwords
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Notiz".to_owned()),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .unwrap();

    let all = candidates(&passwords, "").await.unwrap();
    assert_eq!(
        all.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
        ["Laufwerk: STICK", "Bank"]
    );
    assert!(!format!("{all:?}").contains(MARKER));
    let found = candidates(&passwords, "bank").await.unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn a_typed_password_becomes_one_entry_of_the_user_s() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    remember_typed(
        &passwords,
        KEY,
        " Laufwerk: STICK ",
        &DriveSecret::new(MARKER.to_owned()),
    )
    .await
    .unwrap();

    let found = matching_entries(&passwords, KEY).await.unwrap();
    assert_eq!(found.len(), 1);
    let header = header(&passwords, &found[0].item_id).await;
    assert_eq!(header.title.as_deref(), Some("Laufwerk: STICK"));
    assert_eq!(header.url, Some(entry_url(KEY)));
    assert_eq!(tags(&header), [TAG]);
    assert_eq!(header.owner, None, "an ordinary entry, it syncs");
}

#[tokio::test]
async fn a_picked_entry_without_an_address_gets_the_drive_s() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    let id = entry(&passwords, "Stick", None).await;
    link_entry(&passwords, &id, KEY, false).await.unwrap();
    let header = header(&passwords, &id).await;
    assert_eq!(header.url, Some(entry_url(KEY)));
    assert_eq!(tags(&header), [TAG]);
}

#[tokio::test]
async fn an_entry_with_another_address_is_replaced_only_when_asked() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    let id = entry(&passwords, "Bank", Some("https://bank.example")).await;
    assert_eq!(
        link_entry(&passwords, &id, KEY, false).await,
        Err(DriveError::UrlTaken)
    );
    let unchanged = header(&passwords, &id).await;
    assert_eq!(unchanged.url.as_deref(), Some("https://bank.example"));
    assert!(unchanged.tags.is_empty());

    link_entry(&passwords, &id, KEY, true).await.unwrap();
    let replaced = header(&passwords, &id).await;
    assert_eq!(replaced.url, Some(entry_url(KEY)));
    assert_eq!(tags(&replaced), [TAG]);
}

#[tokio::test]
async fn an_entry_already_linked_stays_as_it_is() {
    let (_dir, db) = vault();
    let passwords = PasswordsService::new(db);
    let id = entry(&passwords, "Stick", Some(&entry_url(KEY))).await;
    let before = header(&passwords, &id).await;
    link_entry(&passwords, &id, KEY, false).await.unwrap();
    assert_eq!(header(&passwords, &id).await, before);
}

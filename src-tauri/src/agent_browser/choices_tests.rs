//! The tracker and DNS choices as rows of the vault (spec 084, T050–T053) against real temporary
//! vaults.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::choices::{
    dns, import_legacy, read_dns, read_trackers, set_dns, set_trackers, write_dns, write_trackers,
    Imported, KEY_DNS, KEY_TRACKERS,
};
use super::dns::{DnsMode, DnsProvider, DnsSettings};
use super::filter_fetch::ListFetcher;
use super::filter_lists::FilterLists;
use super::filter_settings::{CustomList, TrackerSettings};
use crate::identity::VAULT_SCOPE_UUID;
use crate::remote_storage::test_support::vault;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// Never reaches the network: every list is unavailable.
struct Offline;

#[async_trait]
impl ListFetcher for Offline {
    async fn get(&self, _url: &str) -> Result<String, String> {
        Err("offline".to_owned())
    }
}

fn mine() -> TrackerSettings {
    TrackerSettings {
        enabled: false,
        off: ["easylist".to_owned()].into(),
        custom: vec![CustomList {
            id: "mine".to_owned(),
            name: "Mine".to_owned(),
            url: "https://lists.example/mine.txt".to_owned(),
            enabled: true,
        }],
    }
}

fn secure() -> DnsSettings {
    DnsSettings {
        mode: DnsMode::Secure,
        provider: DnsProvider::Cloudflare,
        custom: String::new(),
    }
}

/// The scope a row of `key` is stored under, as its raw text.
fn stored_scope(db: &VaultDb, key: &'static str) -> Option<String> {
    db.read_blocking(move |q| {
        q.query_row(
            "SELECT vault_device_uuid FROM preferences WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
    })
    .unwrap()
}

fn put_raw(db: &VaultDb, scope: PrefScope, key: &'static str, value: &'static str) {
    db.write_blocking(move |tx| preferences::insert_or_update(tx, scope, key, value))
        .unwrap();
}

fn trackers_row(db: &VaultDb) -> Option<TrackerSettings> {
    db.read_blocking(|q| read_trackers(q)).unwrap()
}

fn dns_row(db: &VaultDb, device: Uuid) -> Option<DnsSettings> {
    db.read_blocking(move |q| read_dns(q, device)).unwrap()
}

fn legacy_files(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let trackers = dir.join("settings.json");
    let dns = dir.join("dns.json");
    std::fs::write(&trackers, serde_json::to_string(&mine()).unwrap()).unwrap();
    std::fs::write(&dns, serde_json::to_string(&secure()).unwrap()).unwrap();
    (trackers, dns)
}

#[test]
fn the_tracker_choice_is_one_row_for_the_whole_vault() {
    let (_dir, db) = vault();
    assert_eq!(trackers_row(&db), None);

    db.write_blocking(|tx| write_trackers(tx, &mine())).unwrap();

    assert_eq!(trackers_row(&db), Some(mine()));
    assert_eq!(
        stored_scope(&db, KEY_TRACKERS).as_deref(),
        Some(VAULT_SCOPE_UUID.to_string().as_str())
    );
}

#[test]
fn the_dns_choice_is_one_row_of_this_device_that_no_other_device_reads() {
    let (_dir, db) = vault();
    let device = db.device_id();
    assert_eq!(dns_row(&db, device), None);

    db.write_blocking(move |tx| write_dns(tx, device, &secure()))
        .unwrap();

    assert_eq!(dns_row(&db, device), Some(secure()));
    assert_eq!(
        stored_scope(&db, KEY_DNS).as_deref(),
        Some(device.to_string().as_str())
    );
    assert_eq!(dns_row(&db, Uuid::new_v4()), None, "another device");
}

#[test]
fn a_row_that_cannot_be_used_counts_as_absent() {
    let (_dir, db) = vault();
    let device = db.device_id();
    for broken in [
        "{ not json",
        r#"{"enabled":false,"custom":[{"id":"x","name":"x","url":"http://insecure.example/a"}]}"#,
    ] {
        put_raw(&db, PrefScope::Vault, KEY_TRACKERS, broken);
        assert_eq!(trackers_row(&db), None, "{broken}");
    }
    for broken in [
        "[]",
        r#"{"mode":"secure","provider":"custom","custom":"http://insecure.example/"}"#,
    ] {
        put_raw(&db, PrefScope::Device(device), KEY_DNS, broken);
        assert_eq!(dns_row(&db, device), None, "{broken}");
    }
}

#[tokio::test]
async fn the_dns_choice_defaults_and_is_kept_in_the_vault() {
    let (_dir, db) = vault();
    assert_eq!(dns(&db).await.unwrap(), DnsSettings::default());

    let kept = set_dns(&db, secure()).await.unwrap();

    assert_eq!(kept, secure());
    assert_eq!(dns(&db).await.unwrap(), secure());
}

#[tokio::test]
async fn a_dns_choice_that_is_not_valid_changes_nothing() {
    let (_dir, db) = vault();
    let bad = DnsSettings {
        mode: DnsMode::Secure,
        provider: DnsProvider::Custom,
        custom: "http://insecure.example/".to_owned(),
    };

    assert!(set_dns(&db, bad).await.is_err());

    assert_eq!(dns_row(&db, db.device_id()), None);
}

#[tokio::test]
async fn a_tracker_choice_is_checked_then_kept_in_the_vault_and_applied() {
    let (_dir, db) = vault();
    let cache = tempfile::tempdir().unwrap();
    let lists = FilterLists::new(cache.path().to_path_buf(), Arc::new(Offline));
    let mut choice = mine();
    choice.custom[0].id = String::new();

    let kept = set_trackers(&lists, &db, choice).await.unwrap();

    assert!(!kept.custom[0].id.is_empty(), "a new list gets an id");
    assert_eq!(trackers_row(&db), Some(kept.clone()));
    assert_eq!(lists.settings(), kept);
}

#[tokio::test]
async fn a_tracker_choice_that_is_not_valid_changes_nothing() {
    let (_dir, db) = vault();
    let cache = tempfile::tempdir().unwrap();
    let lists = FilterLists::new(cache.path().to_path_buf(), Arc::new(Offline));
    let mut choice = mine();
    choice.custom[0].url = "http://lists.example/mine.txt".to_owned();

    assert!(set_trackers(&lists, &db, choice).await.is_err());

    assert_eq!(trackers_row(&db), None);
    assert_eq!(lists.settings(), TrackerSettings::default());
}

#[tokio::test]
async fn the_old_files_become_rows_of_the_first_vault_and_go() {
    let (_vault_dir, db) = vault();
    let files = tempfile::tempdir().unwrap();
    let (trackers_file, dns_file) = legacy_files(files.path());

    let imported = import_legacy(&db, db.device_id(), &trackers_file, &dns_file)
        .await
        .unwrap();

    assert_eq!(
        imported,
        Imported {
            trackers: true,
            dns: true
        }
    );
    assert_eq!(trackers_row(&db), Some(mine()));
    assert_eq!(dns_row(&db, db.device_id()), Some(secure()));
    assert!(!trackers_file.exists() && !dns_file.exists());
}

#[tokio::test]
async fn the_second_vault_starts_from_the_defaults() {
    let (_first_dir, first) = vault();
    let (_second_dir, second) = vault();
    let files = tempfile::tempdir().unwrap();
    let (trackers_file, dns_file) = legacy_files(files.path());

    import_legacy(&first, first.device_id(), &trackers_file, &dns_file)
        .await
        .unwrap();
    let again = import_legacy(&second, second.device_id(), &trackers_file, &dns_file)
        .await
        .unwrap();

    assert_eq!(again, Imported::default());
    assert_eq!(trackers_row(&second), None);
    assert_eq!(dns_row(&second, second.device_id()), None);
}

#[tokio::test]
async fn a_vault_with_a_row_never_reads_the_old_file_and_leaves_it_for_another_vault() {
    let (_vault_dir, db) = vault();
    let device = db.device_id();
    let own_trackers = TrackerSettings::default();
    let own_dns = DnsSettings::off();
    {
        let (trackers, dns) = (own_trackers.clone(), own_dns.clone());
        db.write_blocking(move |tx| {
            write_trackers(tx, &trackers)?;
            write_dns(tx, device, &dns)
        })
        .unwrap();
    }
    let files = tempfile::tempdir().unwrap();
    let (trackers_file, dns_file) = legacy_files(files.path());

    let imported = import_legacy(&db, device, &trackers_file, &dns_file)
        .await
        .unwrap();

    assert_eq!(imported, Imported::default());
    assert_eq!(trackers_row(&db), Some(own_trackers));
    assert_eq!(dns_row(&db, device), Some(own_dns));
    assert!(
        trackers_file.exists() && dns_file.exists(),
        "kept for another vault"
    );
}

#[tokio::test]
async fn a_row_of_one_kind_does_not_stop_the_import_of_the_other() {
    let (_vault_dir, db) = vault();
    let device = db.device_id();
    db.write_blocking(|tx| write_trackers(tx, &TrackerSettings::default()))
        .unwrap();
    let files = tempfile::tempdir().unwrap();
    let (trackers_file, dns_file) = legacy_files(files.path());

    let imported = import_legacy(&db, device, &trackers_file, &dns_file)
        .await
        .unwrap();

    assert_eq!(
        imported,
        Imported {
            trackers: false,
            dns: true
        }
    );
    assert_eq!(trackers_row(&db), Some(TrackerSettings::default()));
    assert_eq!(dns_row(&db, device), Some(secure()));
    assert!(trackers_file.exists() && !dns_file.exists());
}

#[tokio::test]
async fn a_file_that_cannot_be_used_is_left_alone() {
    let (_vault_dir, db) = vault();
    let files = tempfile::tempdir().unwrap();
    let trackers_file = files.path().join("settings.json");
    let dns_file = files.path().join("dns.json");
    std::fs::write(&trackers_file, "{ not json").unwrap();

    let imported = import_legacy(&db, db.device_id(), &trackers_file, &dns_file)
        .await
        .unwrap();

    assert_eq!(imported, Imported::default());
    assert_eq!(
        std::fs::read_to_string(&trackers_file).unwrap(),
        "{ not json"
    );
    assert_eq!(trackers_row(&db), None);
}

//! Devices for the life-cycle tests (spec 017, T076, T090): each one on the real pull path of
//! `sync::test_support`, following the registry with [`reconcile`], and bundles of one test
//! extension `tasks`.

// The tests read and change rows directly to observe and disturb each device.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use ed25519_dalek::SigningKey;
use haex_bundle::jcs::parse_restricted;
use haex_bundle::{build_archive, Entry};
use haex_crdt::rusqlite::params;
use sha2::{Digest, Sha256};

use super::*;
use crate::extensions::host::ExtensionHost;
use crate::extensions::registry::install::install;
use crate::storage::known_devices;
use crate::storage::query;
use crate::sync::test_support::Device;
use crate::vault_gate::VaultGate;

/// Signs the bundles of these tests only; derived from a public seed, never trusted elsewhere.
pub(super) fn key() -> SigningKey {
    let seed = Sha256::digest(b"HaexVault spec 017 lifecycle tests only, never use for signing");
    SigningKey::from_bytes(&seed.into())
}

pub(super) fn public_key() -> String {
    key()
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The table of the test extension.
pub(super) fn items() -> String {
    format!("{}__tasks__items", public_key())
}

pub(super) const INIT: &str = "CREATE TABLE `{t}` (`id` text PRIMARY KEY NOT NULL, `label` text);";
pub(super) const TAG: &str = "ALTER TABLE `{t}` ADD `tag` text;";

/// The extension `tasks` at `version` with `migrations` (`{t}` stands for its table).
pub(super) fn bundle(version: &str, migrations: &[&str]) -> Vec<u8> {
    let manifest = parse_restricted(&format!(
        r#"{{"name":"tasks","version":"{version}","displayName":"Tasks","migrationsDir":"db"}}"#
    ))
    .expect("manifest");
    let mut files = vec![Entry {
        path: "index.html".into(),
        data: format!("<!doctype html><title>Tasks {version}</title>").into_bytes(),
    }];
    for (i, sql) in migrations.iter().enumerate() {
        files.push(Entry {
            path: format!("db/{i:04}_step.sql"),
            data: sql.replace("{t}", &items()).into_bytes(),
        });
    }
    build_archive(files, manifest, &key()).expect("bundle")
}

/// One device with what the extension host keeps for it.
pub(super) struct Node {
    pub(super) device: Device,
    pub(super) vault: VaultDb,
    pub(super) host: ExtensionHost,
    pub(super) me: Uuid,
}

impl Node {
    pub(super) fn new() -> Self {
        let device = Device::new();
        let vault = VaultGate::new()
            .vault_db(Arc::new(device.db().clone()))
            .expect("vault");
        let me = vault
            .read_blocking(|q| known_devices::list_devices(q))
            .expect("devices")[0]
            .vault_device_uuid;
        Self {
            device,
            vault,
            host: ExtensionHost::default(),
            me,
        }
    }

    pub(super) fn install(&self, bytes: &[u8]) -> Uuid {
        install(&self.vault, bytes, vec![], false, self.me, now())
            .expect("install")
            .ids
            .extension_id
    }

    pub(super) fn follow(&self) -> Vec<ExtensionStatusChanged> {
        reconcile(&self.vault, &self.host, self.me, now()).expect("reconcile")
    }

    pub(super) fn pull(&self, from: &Node) {
        self.device.pull_from(&from.device);
    }

    pub(super) fn status(&self, extension: Uuid) -> Option<(String, Option<String>)> {
        let id = crate::extensions::ids::device_status_id(extension, self.me).to_string();
        query::read(self.device.db(), |r| {
            r.query_row(
                "SELECT status, bundle_id FROM extension_device_status WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .expect("status")
    }

    pub(super) fn write(&self, sql: &str) {
        let sql = sql.replace("{t}", &items());
        self.device
            .db()
            .write(|tx| tx.execute(&sql, &[]).map(drop))
            .expect("write");
    }

    pub(super) fn count(&self, sql: &str) -> Option<i64> {
        let sql = sql.replace("{t}", &items());
        query::read(self.device.db(), |r| {
            r.query_row(&sql, &[], |row| row.get(0))
        })
        .ok()?
    }

    pub(super) fn has_items(&self) -> bool {
        let table = items();
        query::read(self.device.db(), |r| {
            r.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1",
                params![table],
                |row| row.get::<_, i64>(0),
            )
        })
        .expect("schema")
            == Some(1)
    }

    pub(super) fn journal(&self) -> i64 {
        self.count("SELECT COUNT(*) FROM extension_migrations_applied_no_sync")
            .unwrap_or(0)
    }

    pub(super) fn parked(&self) -> i64 {
        self.count("SELECT COUNT(*) FROM sync_parked_groups_no_sync")
            .unwrap_or(0)
    }
}

pub(super) fn now() -> i64 {
    crate::passwords::clock::unix_millis(std::time::SystemTime::now())
}

pub(super) fn ready(node: &Node, extension: Uuid) -> bool {
    node.status(extension).is_some_and(|(s, _)| s == "ready")
}

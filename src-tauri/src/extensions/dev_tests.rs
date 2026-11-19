// The tests create tables and read registry rows directly.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use serde_json::{json, Value};

use super::*;
use crate::extensions::bridge::dispatch::{call, CallContext, Emit};
use crate::extensions::host::ExtensionHost;
use crate::extensions::registry::install::install;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::sync::replica::synced_tables;
use crate::vault_gate::VaultGate;

/// A key only these tests use, derived from a public seed.
fn key() -> String {
    use sha2::{Digest, Sha256};
    let seed = Sha256::digest(b"holzi spec 017 developer mode tests only");
    ed25519_dalek::SigningKey::from_bytes(&seed.into())
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Silent;

impl Emit for Silent {
    fn emit(&self, _event: &str, _payload: Value) {}
}

struct Setup {
    _dir: tempfile::TempDir,
    vault: VaultDb,
    device: Uuid,
}

fn setup() -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    Setup {
        _dir: dir,
        vault,
        device,
    }
}

impl Setup {
    fn mode(&self, enabled: bool) {
        let device = self.device;
        self.vault
            .write_blocking(move |tx| set_mode(tx, device, enabled).map_err(Into::into))
            .unwrap();
    }

    fn count(&self, sql: &str) -> i64 {
        let sql = sql.to_owned();
        self.vault
            .read_blocking(move |q| q.query_row(&sql, &[], |r| r.get::<_, i64>(0)))
            .unwrap()
            .unwrap_or(0)
    }

    fn sql(&self, sql: &str) {
        let sql = sql.to_owned();
        self.vault
            .write_blocking(move |tx| tx.execute(&sql, &[]).map(drop))
            .unwrap();
    }
}

/// A project folder with `haextension.config.json` and `haextension/manifest.json`.
fn project(manifest: &Value, host: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("haextension.config.json"),
        json!({ "dev": { "host": host, "port": 5199 } }).to_string(),
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("haextension")).unwrap();
    std::fs::write(
        dir.path().join("haextension/manifest.json"),
        manifest.to_string(),
    )
    .unwrap();
    dir
}

fn manifest(key: &str, name: &str) -> Value {
    json!({
        "name": name,
        "version": "0.1.0",
        "publicKey": key,
        "displayName": "Draft",
        "permissions": { "filesystem": [{ "target": "/tmp/haex-dev", "operation": "read" }] }
    })
}

fn reason(outcome: Result<impl std::fmt::Debug>) -> String {
    match outcome {
        Err(HaexError::ExtensionInstall { reason }) => reason,
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn a_project_names_its_loopback_server_and_nothing_else() {
    let dir = project(&manifest(&key(), "draft"), "localhost");
    let read = read_project(dir.path()).unwrap();
    assert_eq!(read.url, "http://localhost:5199");
    assert_eq!(read.manifest.name.as_str(), "draft");

    for host in ["example.com", "::1", "192.168.1.2"] {
        let dir = project(&manifest(&key(), "draft"), host);
        assert_eq!(reason(read_project(dir.path())), "dev_server_not_loopback");
    }
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(reason(read_project(empty.path())), "dev_manifest_missing");
    let not_a_key = project(&manifest(&"0".repeat(64), "draft"), "localhost");
    assert_eq!(
        reason(read_project(not_a_key.path())),
        "dev_public_key_invalid"
    );
}

#[test]
fn the_name_comes_from_package_json_as_in_the_sdk() {
    // As `haex init` writes it: no name or version in the manifest.
    let mut bare = manifest(&key(), "unused");
    bare.as_object_mut().unwrap().remove("name");
    bare.as_object_mut().unwrap().remove("version");
    let dir = project(&bare, "localhost");
    assert_eq!(reason(read_project(dir.path())), "manifest_invalid");
    std::fs::write(
        dir.path().join("package.json"),
        json!({ "name": "draft", "version": "1.2.3", "author": { "name": "x" } }).to_string(),
    )
    .unwrap();
    let read = read_project(dir.path()).unwrap();
    assert_eq!(read.manifest.name.as_str(), "draft");
    assert_eq!(read.manifest.version.to_string(), "1.2.3");
    assert_eq!(read.manifest.author, None, "not a text");

    // The package's name wins over the manifest's, the manifest's version over the package's.
    let dir = project(&manifest(&key(), "other"), "localhost");
    std::fs::write(
        dir.path().join("package.json"),
        json!({ "name": "draft", "version": "9.9.9" }).to_string(),
    )
    .unwrap();
    let read = read_project(dir.path()).unwrap();
    assert_eq!(read.manifest.name.as_str(), "draft");
    assert_eq!(read.manifest.version.to_string(), "0.1.0");
}

#[test]
fn a_project_that_names_another_prefix_now_must_load_again() {
    let s = setup();
    s.mode(true);
    let dir = project(&manifest(&key(), "draft"), "localhost");
    let id = confirm(&s.vault, dir.path(), vec![], s.device, 1).unwrap();
    let registered = s
        .vault
        .read_blocking(move |q| registration(q, id).map_err(Into::into))
        .unwrap()
        .unwrap();

    // A new port is taken up without loading again.
    std::fs::write(
        dir.path().join("haextension.config.json"),
        json!({ "dev": { "host": "localhost", "port": 5200 } }).to_string(),
    )
    .unwrap();
    assert_eq!(
        current_project(&registered).unwrap().url,
        "http://localhost:5200"
    );

    std::fs::write(
        dir.path().join("package.json"),
        json!({ "name": "renamed" }).to_string(),
    )
    .unwrap();
    assert_eq!(reason(current_project(&registered)), "dev_project_changed");
    let refused = call(&dev_frame(&s, id), "extension_get_info", &Value::Null).unwrap_err();
    assert!(refused.message.contains("load it again"), "{refused:?}");
}

#[test]
fn loading_needs_developer_mode() {
    let s = setup();
    let dir = project(&manifest(&key(), "draft"), "127.0.0.1");
    assert_eq!(
        reason(confirm(&s.vault, dir.path(), vec![], s.device, 1)),
        "dev_mode_off"
    );
    s.mode(true);
    confirm(&s.vault, dir.path(), vec![], s.device, 1).unwrap();
    assert_eq!(s.count("SELECT COUNT(*) FROM dev_extensions_no_sync"), 1);
}

#[test]
fn an_installed_extension_or_foreign_tables_of_the_prefix_refuse_loading() {
    let s = setup();
    s.mode(true);
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-notes-like.xt"),
    )
    .unwrap();
    let installed = install(&s.vault, &bytes, vec![], false, s.device, 1)
        .unwrap()
        .ids
        .extension_id;
    let fixture_key: String = s
        .vault
        .read_blocking(move |q| {
            q.query_row(
                "SELECT public_key FROM extensions WHERE id = ?1",
                params![installed.to_string()],
                |r| r.get(0),
            )
        })
        .unwrap()
        .unwrap();
    let same = project(&manifest(&fixture_key, "notes-like"), "localhost");
    assert_eq!(
        reason(confirm(&s.vault, same.path(), vec![], s.device, 1)),
        "dev_installed_conflict"
    );
    // Also after a removal that kept the data.
    crate::extensions::registry::remove::remove(&s.vault, installed, false, 2).unwrap();
    assert_eq!(
        reason(confirm(&s.vault, same.path(), vec![], s.device, 3)),
        "dev_installed_conflict"
    );

    s.sql(&format!(
        "CREATE TABLE \"{}__other__pages\" (id TEXT PRIMARY KEY)",
        key()
    ));
    let tables = project(&manifest(&key(), "other"), "localhost");
    assert_eq!(
        reason(confirm(&s.vault, tables.path(), vec![], s.device, 4)),
        "dev_tables_exist"
    );
}

/// The context of a frame of the development version `id`.
fn dev_frame(s: &Setup, id: Uuid) -> CallContext {
    let host = Arc::new(ExtensionHost::default());
    CallContext {
        db: s.vault.clone(),
        session: host.frames.open_dev(id, "tab"),
        host,
        device: s.device,
        emitter: Arc::new(Silent),
    }
}

#[test]
fn everything_of_a_development_version_stays_on_this_device_and_goes_with_unloading() {
    let s = setup();
    s.mode(true);
    let dir = project(&manifest(&key(), "draft"), "localhost");
    let choice = PermissionChoice {
        kind: "filesystem".into(),
        action: "read".into(),
        target: "/tmp/haex-dev".into(),
        granted: true,
    };
    let id = confirm(&s.vault, dir.path(), vec![choice], s.device, 1).unwrap();
    assert_eq!(s.count("SELECT COUNT(*) FROM extension_permissions"), 0);
    assert_eq!(
        s.count("SELECT COUNT(*) FROM dev_extension_permissions_no_sync"),
        1
    );

    let frame = dev_frame(&s, id);
    let table = format!("{}__draft__notes", key());
    let registered = call(
        &frame,
        "extension_database_register_migrations",
        &json!({ "migrations": [{
            "name": "0000_init",
            "sql": format!("CREATE TABLE `{table}` (`id` text PRIMARY KEY NOT NULL, `body` text);")
        }] }),
    )
    .unwrap();
    assert_eq!(registered["appliedCount"], 1);
    call(
        &frame,
        "extension_database_execute",
        &json!({ "query": format!("INSERT INTO \"{table}\" (id, body) VALUES ('n', 'x')"), "params": [] }),
    )
    .unwrap();
    call(
        &frame,
        "extension_web_storage_set_item",
        &json!({ "key": "zoom", "value": "2" }),
    )
    .unwrap();
    assert_eq!(
        call(
            &frame,
            "extension_web_storage_get_item",
            &json!({ "key": "zoom" })
        )
        .unwrap(),
        json!("2")
    );
    assert_eq!(s.count("SELECT COUNT(*) FROM extension_kv"), 0);
    assert_eq!(s.count("SELECT COUNT(*) FROM dev_extension_kv_no_sync"), 1);

    let synced = s.vault.read_blocking(|q| synced_tables(q)).unwrap();
    assert!(!synced.contains(&table), "not synced");
    assert_eq!(
        s.count(&format!(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = '{table}' AND instr(sql, 'haex_') > 0"
        )),
        0,
        "no CRDT columns"
    );

    unload(&s.vault, id).unwrap();
    assert_eq!(
        s.count(&format!(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = '{table}'"
        )),
        0
    );
    for table in [
        "dev_extensions_no_sync",
        "dev_extension_permissions_no_sync",
        "dev_extension_kv_no_sync",
        "extension_migrations_applied_no_sync",
    ] {
        assert_eq!(
            s.count(&format!("SELECT COUNT(*) FROM {table}")),
            0,
            "{table}"
        );
    }
}

#[test]
fn a_signed_install_of_the_prefix_waits_for_the_unload() {
    let s = setup();
    s.mode(true);
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-notes-like.xt"),
    )
    .unwrap();
    // The key of the fixture, from an install that is removed with its data right away.
    let installed = install(&s.vault, &bytes, vec![], false, s.device, 1)
        .unwrap()
        .ids
        .extension_id;
    let key: String = s
        .vault
        .read_blocking(move |q| {
            q.query_row(
                "SELECT public_key FROM extensions WHERE id = ?1",
                params![installed.to_string()],
                |r| r.get(0),
            )
        })
        .unwrap()
        .unwrap();
    s.sql("DELETE FROM extensions");
    let dir = project(&manifest(&key, "notes-like"), "localhost");
    let id = confirm(&s.vault, dir.path(), vec![], s.device, 2).unwrap();

    assert_eq!(
        reason(install(&s.vault, &bytes, vec![], false, s.device, 3)),
        "dev_prefix_conflict"
    );
    unload(&s.vault, id).unwrap();
    install(&s.vault, &bytes, vec![], false, s.device, 4).unwrap();
}

#[test]
fn a_frame_of_a_development_version_runs_only_while_developer_mode_is_on() {
    let s = setup();
    s.mode(true);
    let dir = project(&manifest(&key(), "draft"), "localhost");
    let id = confirm(&s.vault, dir.path(), vec![], s.device, 1).unwrap();
    let frame = dev_frame(&s, id);
    let info = call(&frame, "extension_get_info", &Value::Null).unwrap();
    assert_eq!(info["name"], "draft");
    assert_eq!(info["displayName"], "Draft");

    s.mode(false);
    let refused = call(&frame, "extension_get_info", &Value::Null).unwrap_err();
    assert_eq!(refused.code.as_u16(), 8002);
}

#[test]
fn with_developer_mode_off_a_loaded_version_stays_listed_for_unloading_but_not_enabled() {
    let s = setup();
    s.mode(true);
    let dir = project(&manifest(&key(), "draft"), "localhost");
    let id = confirm(&s.vault, dir.path(), vec![], s.device, 1).unwrap();
    let device = s.device;
    let listed = |s: &Setup| {
        s.vault
            .read_blocking(move |q| {
                crate::extensions::registry::list::list(q, device).map_err(Into::into)
            })
            .unwrap()
            .into_iter()
            .find(|e| e.id == id.to_string())
            .map(|e| (e.dev, e.enabled))
    };
    assert_eq!(listed(&s), Some((true, true)));

    s.mode(false);
    assert_eq!(listed(&s), Some((true, false)));
    unload(&s.vault, id).unwrap();
    assert_eq!(listed(&s), None);
}

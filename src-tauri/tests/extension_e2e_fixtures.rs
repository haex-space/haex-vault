//! The extension bundles of the e2e scenes of specs 017, 018, 055 and 057 (`scripts/e2e/scenarios/extension-*.test.ts`),
//! built with the one implementation of the bundle format (`haex-bundle`) and a test key derived
//! from a public seed: it signs these fixtures only and must never be trusted elsewhere.
//!
//! The committed `.xt` files must equal what this builds; `HOLZI_WRITE_E2E_FIXTURES=1` rewrites
//! them after a change.

use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use haex_bundle::jcs::parse_restricted;
use haex_bundle::{build_archive, Entry};
use sha2::{Digest, Sha256};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extension_e2e")
}

fn test_key() -> SigningKey {
    let seed =
        Sha256::digest(b"holzi spec 017 e2e fixtures only, never use for real signing: probe");
    SigningKey::from_bytes(&seed.into())
}

/// The probe at `version`; 1.1.0 adds a column (the update of `extension-two-devices`). It
/// declares reading the contacts of the group "Arbeit" (spec 051, `extension-contacts`).
fn probe(version: &str) -> Vec<u8> {
    let key = test_key();
    let public_key: String = key
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let table = format!("{public_key}__probe__items");
    let manifest = parse_restricted(&format!(
        r#"{{"name":"probe","version":"{version}","displayName":"Probe","migrationsDir":"db",
            "permissions":{{"filesystem":[{{"target":"/tmp/holzi-probe","operation":"read"}}],
            "contacts":[{{"target":"Arbeit","operation":"read"}}]}}}}"#
    ))
    .expect("manifest");
    let page = std::fs::read(dir().join("probe.html")).expect("probe page");
    let module = std::fs::read(dir().join("probe.js")).expect("probe module");
    let mut files = vec![
        Entry {
            path: "index.html".into(),
            data: page,
        },
        Entry {
            path: "probe.js".into(),
            data: module,
        },
        Entry {
            path: "db/0000_init.sql".into(),
            data: format!("CREATE TABLE `{table}` (`id` text PRIMARY KEY NOT NULL, `label` text);")
                .into_bytes(),
        },
    ];
    if version != "1.0.0" {
        files.push(Entry {
            path: "db/0001_tag.sql".into(),
            data: format!("ALTER TABLE `{table}` ADD `tag` text;").into_bytes(),
        });
    }
    build_archive(files, manifest, &key).expect("probe bundle")
}

/// The test extension of spec 018 (research R13): sources and `manifest.json` in `agent-tools/`,
/// its code bundled with the released vault-sdk by `agent-tools/build.sh`.
fn agent_tools() -> Vec<u8> {
    let key = test_key();
    let public_key: String = key
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let source = dir().join("agent-tools");
    let read = |name: &str| std::fs::read(source.join(name)).expect("agent-tools source");
    let manifest =
        parse_restricted(&String::from_utf8(read("manifest.json")).expect("manifest text"))
            .expect("manifest");
    let table = format!("{public_key}__agent-tools__entries");
    let files = vec![
        Entry {
            path: "index.html".into(),
            data: read("index.html"),
        },
        Entry {
            path: "app.js".into(),
            data: read("app.js"),
        },
        Entry {
            path: "db/0000_init.sql".into(),
            data: format!("CREATE TABLE `{table}` (`id` text PRIMARY KEY NOT NULL, `text` text);")
                .into_bytes(),
        },
    ];
    build_archive(files, manifest, &key).expect("agent-tools bundle")
}

/// The test extension of spec 055 (T065): sources and `manifest.json` in `model-jobs/`, its code
/// bundled with the released vault-sdk by `model-jobs/build.sh`. It keeps no tables.
fn model_jobs() -> Vec<u8> {
    let key = test_key();
    let source = dir().join("model-jobs");
    let read = |name: &str| std::fs::read(source.join(name)).expect("model-jobs source");
    let manifest =
        parse_restricted(&String::from_utf8(read("manifest.json")).expect("manifest text"))
            .expect("manifest");
    let files = vec![
        Entry {
            path: "index.html".into(),
            data: read("index.html"),
        },
        Entry {
            path: "app.js".into(),
            data: read("app.js"),
        },
    ];
    build_archive(files, manifest, &key).expect("model-jobs bundle")
}

/// The test extension of spec 057 (T069): sources and `manifest.json` in `peers/`, its code
/// bundled with the released vault-sdk by `peers/build.sh`. It keeps no tables.
fn peers() -> Vec<u8> {
    let key = test_key();
    let source = dir().join("peers");
    let read = |name: &str| std::fs::read(source.join(name)).expect("peers source");
    let manifest =
        parse_restricted(&String::from_utf8(read("manifest.json")).expect("manifest text"))
            .expect("manifest");
    let files = vec![
        Entry {
            path: "index.html".into(),
            data: read("index.html"),
        },
        Entry {
            path: "app.js".into(),
            data: read("app.js"),
        },
    ];
    build_archive(files, manifest, &key).expect("peers bundle")
}

#[test]
fn the_committed_e2e_bundles_are_what_this_builds() {
    let bundles = [
        ("probe.xt", probe("1.0.0")),
        ("probe-v2.xt", probe("1.1.0")),
        ("agent-tools.xt", agent_tools()),
        ("model-jobs.xt", model_jobs()),
        ("peers.xt", peers()),
    ];
    for (file, built) in bundles {
        let path = dir().join(file);
        if std::env::var_os("HOLZI_WRITE_E2E_FIXTURES").is_some() {
            std::fs::write(&path, &built).expect("write fixture");
        }
        let committed =
            std::fs::read(&path).expect("fixture (HOLZI_WRITE_E2E_FIXTURES=1 builds it)");
        eprintln!(
            "{file}: built {:x}, committed {:x}",
            Sha256::digest(&built),
            Sha256::digest(&committed)
        );
        assert!(
            committed == built,
            "{file} is stale; rebuild with HOLZI_WRITE_E2E_FIXTURES=1"
        );
    }
}

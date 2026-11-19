//! Spec 017, T024: every shared bundle test vector (`tests/fixtures/extension_bundles/`, copied
//! from the vault-sdk, see `SOURCE.md`) gives through holzi's adapter exactly the outcome
//! `expected.json` names. The rules themselves are tested once, in the crate `haex-bundle`; this
//! test pins the integration, the error mapping and the pinned crate revision.

use std::path::{Path, PathBuf};

use haex_lib::error::HaexError;
use haex_lib::extensions::bundle::{verify_bundle, Manifest};

fn vectors() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extension_bundles")
}

#[test]
fn every_vector_gives_its_expected_outcome() {
    let expected: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&std::fs::read(vectors().join("expected.json")).unwrap()).unwrap();
    let on_disk = std::fs::read_dir(vectors())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|x| x == "xt")
        })
        .count();
    assert_eq!(expected.len(), on_disk, "every vector is listed");

    let mut failures = Vec::new();
    for (name, outcome) in &expected {
        let bytes = std::fs::read(vectors().join(name)).unwrap();
        let actual = match verify_bundle(&bytes) {
            Ok(bundle) => {
                Manifest::from_verified(&bundle).expect("a verified manifest has a typed view");
                serde_json::json!({ "valid": true })
            }
            Err(rejection) => {
                let mut value = serde_json::json!({ "valid": false, "kind": rejection.kind });
                if outcome.get("path").is_some() {
                    value["path"] = serde_json::json!(rejection.path);
                }
                value
            }
        };
        if &actual != outcome {
            failures.push(format!("{name}: expected {outcome}, got {actual}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_refused_bundle_becomes_an_install_error_naming_the_kind() {
    let bytes = std::fs::read(vectors().join("legacy-format.xt")).unwrap();
    let error = HaexError::from(verify_bundle(&bytes).unwrap_err());
    assert!(matches!(
        error,
        HaexError::ExtensionInstall { ref reason } if reason == "legacy_signature_format"
    ));
}

#[test]
fn the_notes_like_vector_yields_files_and_migrations_in_journal_order() {
    let bundle =
        verify_bundle(&std::fs::read(vectors().join("good-notes-like.xt")).unwrap()).unwrap();
    assert!(bundle.files.iter().any(|f| f.path == "index.html"));
    let names: Vec<_> = bundle.migrations.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["0000_init", "0001_tags"]);
    assert!(bundle.migrations[0].sql.contains("CREATE TABLE"));
}

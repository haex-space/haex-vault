// These tests read registry rows directly to check what the store wrote.
#![allow(clippy::disallowed_methods)]

use std::path::Path;
use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::*;
use crate::extensions::bundle::verify_bundle;
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::VaultGate;

fn vector(name: &str) -> (VerifiedBundle, Manifest) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/extension_bundles")
        .join(name);
    let bundle = verify_bundle(&std::fs::read(path).unwrap()).unwrap();
    let manifest = Manifest::from_verified(&bundle).unwrap();
    (bundle, manifest)
}

fn store(db: &Database, bundle: &VerifiedBundle, manifest: &Manifest) -> BundleIds {
    let vault = VaultGate::new().vault_db(Arc::new(db.clone())).unwrap();
    store_blobs(&vault, bundle).unwrap();
    let (bundle, manifest) = (bundle.clone(), manifest.clone());
    vault
        .write_blocking(move |tx| {
            write_registry_rows(tx, &bundle, &manifest, 1_000).map_err(Into::into)
        })
        .unwrap()
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

#[test]
fn a_bundle_is_stored_with_its_exact_bytes_files_migrations_and_limits() {
    let (_dir, db) = open_test_vault();
    let (bundle, manifest) = vector("good-notes-like.xt");
    let ids = store(&db, &bundle, &manifest);

    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM extension_bundle_files"),
        bundle.files.len() as i64
    );
    // 200.html and 404.html have the same bytes: one BLOB.
    let distinct = bundle
        .entries
        .iter()
        .map(|e| sha256_hex(&e.data))
        .collect::<std::collections::HashSet<_>>()
        .len() as i64;
    assert_eq!(count(&db, "SELECT COUNT(*) FROM extension_blobs"), distinct);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM extension_migrations"), 2);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM extension_limits"), 1);

    let (manifest_json, state, display): (Vec<u8>, String, Option<String>) = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT b.manifest_json, e.state, e.display_name FROM extension_bundles b \
                 JOIN extensions e ON e.id = b.extension_id WHERE b.id = ?1",
                params![ids.bundle_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .unwrap();
    assert_eq!(manifest_json, bundle.manifest_bytes);
    assert_eq!(state, "installed");
    assert_eq!(display.as_deref(), Some("Notes"));
    let positions: Vec<(String, i64)> = db
        .with_connection(|c| {
            let mut stmt =
                c.prepare("SELECT name, position FROM extension_migrations ORDER BY position")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?;
            Ok(rows)
        })
        .unwrap();
    assert_eq!(
        positions,
        [("0000_init".to_owned(), 0), ("0001_tags".to_owned(), 1)]
    );
}

#[test]
fn storing_the_same_bundle_twice_changes_nothing_and_revives_a_retired_one() {
    let (_dir, db) = open_test_vault();
    let (bundle, manifest) = vector("good-minimal.xt");
    let ids = store(&db, &bundle, &manifest);
    db.write(|tx| {
        tx.execute(
            "UPDATE extension_bundles SET retired = 1 WHERE id = ?1",
            params![ids.bundle_id.to_string()],
        )?;
        Ok(())
    })
    .unwrap();
    let again = store(&db, &bundle, &manifest);
    assert_eq!(again, ids);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM extension_bundles"), 1);
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM extension_bundles WHERE retired = 0"
        ),
        1
    );
}

#[test]
fn files_are_read_by_hash_and_rehashed() {
    let (_dir, db) = open_test_vault();
    let (bundle, manifest) = vector("good-notes-like.xt");
    let ids = store(&db, &bundle, &manifest);
    let index = bundle.file("index.html").unwrap().data.clone();

    let read = |path: &'static str| {
        crate::storage::query::read(&db, move |q| {
            read_verified_file(q, ids.bundle_id, path).map_err(Into::into)
        })
    };
    assert_eq!(read("index.html").unwrap(), Some(index.clone()));
    assert_eq!(read("missing.html").unwrap(), None);

    // A BLOB changed behind the registry's back is refused.
    let hash = sha256_hex(&index);
    db.write(move |tx| {
        tx.execute(
            "UPDATE extension_blobs SET data = ?1 WHERE hash = ?2",
            params![b"<p>changed</p>".to_vec(), hash],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(read("index.html").is_err());
}

#[test]
fn a_file_over_the_limit_is_refused_by_its_stored_length() {
    let (_dir, db) = open_test_vault();
    let (bundle, manifest) = vector("good-notes-like.xt");
    let ids = store(&db, &bundle, &manifest);
    let index = bundle.file("index.html").unwrap().data.clone();
    let read = |max: usize| {
        crate::storage::query::read(&db, move |q| {
            read_verified_file_within(q, ids.bundle_id, "index.html", max).map_err(Into::into)
        })
        .unwrap()
    };
    assert_eq!(read(index.len()), VerifiedFile::Data(index.clone()));
    assert_eq!(read(index.len() - 1), VerifiedFile::TooLarge);

    // The limit is judged by the stored BLOB, not by its hash: a BLOB changed to a longer one is
    // refused as too large without being hashed, one within the limit still fails the hash.
    let hash = sha256_hex(&index);
    let longer = vec![b'x'; index.len() + 10];
    db.write(move |tx| {
        tx.execute(
            "UPDATE extension_blobs SET data = ?1 WHERE hash = ?2",
            params![longer, hash],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(read(index.len()), VerifiedFile::TooLarge);
    assert!(crate::storage::query::read(&db, move |q| {
        read_verified_file_within(q, ids.bundle_id, "index.html", usize::MAX).map_err(Into::into)
    })
    .is_err());
}

#[test]
fn a_stored_bundle_verifies_again_and_detects_missing_or_changed_blobs() {
    let (_dir, db) = open_test_vault();
    let (bundle, manifest) = vector("good-notes-like.xt");
    let ids = store(&db, &bundle, &manifest);
    let state = |db: &Database| {
        crate::storage::query::read(db, |q| {
            verify_stored_bundle(q, ids.bundle_id).map_err(Into::into)
        })
        .unwrap()
    };
    assert!(matches!(state(&db), StoredBundleState::Ready(_)));

    let icon = sha256_hex(&bundle.file("icon.svg").unwrap().data);
    let changed = icon.clone();
    db.write(move |tx| {
        tx.execute(
            "UPDATE extension_blobs SET data = ?1 WHERE hash = ?2",
            params![b"<svg/>".to_vec(), changed],
        )?;
        Ok(())
    })
    .unwrap();
    match state(&db) {
        StoredBundleState::SignatureFailed(rejection) => {
            assert_eq!(rejection.kind, "file_mismatch");
            assert_eq!(rejection.path.as_deref(), Some("icon.svg"));
        }
        other => panic!("expected signature_failed, got {other:?}"),
    }

    db.write(move |tx| {
        tx.execute("DELETE FROM extension_blobs WHERE hash = ?1", params![icon])?;
        Ok(())
    })
    .unwrap();
    assert!(matches!(state(&db), StoredBundleState::Transferring));
}

// These tests change and read registry rows directly to put the extension into each state.
#![allow(clippy::disallowed_methods)]

use std::path::Path;
use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::*;
use crate::extensions::registry::install::install;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::VaultGate;

struct Setup {
    _dir: tempfile::TempDir,
    db: Database,
    vault: VaultDb,
    device: Uuid,
    extension: Uuid,
}

fn installed(vector: &str) -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db.clone())).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles")
            .join(vector),
    )
    .unwrap();
    let extension = install(&vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    Setup {
        _dir: dir,
        db,
        vault,
        device,
        extension,
    }
}

impl Setup {
    fn start(&self) -> Result<Started> {
        start(&self.vault, self.extension, self.device, 2)
    }

    fn status(&self) -> (String, Option<String>) {
        self.db
            .with_connection(|c| {
                Ok(c.query_row(
                    "SELECT status, error FROM extension_device_status WHERE extension_id = ?1",
                    params![self.extension.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .unwrap()
    }

    fn sql(&self, sql: &'static str) {
        self.db
            .write(move |tx| {
                tx.execute(sql, &[])?;
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn a_complete_bundle_starts_and_is_ready_here() {
    let setup = installed("good-notes-like.xt");
    let started = setup.start().unwrap();
    assert_eq!(started.entry, "index.html");
    assert!(started.csp.contains(&prefix(setup.extension)));
    assert!(started.csp.contains("'sha256-"));
    assert_eq!(setup.status(), ("ready".to_owned(), None));
    // A second start writes nothing new.
    setup.start().unwrap();
    assert_eq!(setup.status(), ("ready".to_owned(), None));
}

#[test]
fn a_changed_file_fails_the_signature_and_says_which_kind() {
    let setup = installed("good-notes-like.xt");
    setup.sql("UPDATE extension_blobs SET data = X'00' WHERE size = 97");
    assert!(matches!(
        setup.start(),
        Err(HaexError::ExtensionNotReady { ref status }) if status == "signature_failed"
    ));
    assert_eq!(
        setup.status(),
        (
            "signature_failed".to_owned(),
            Some("file_mismatch".to_owned())
        )
    );
}

#[test]
fn a_missing_blob_is_still_transferring() {
    let setup = installed("good-minimal.xt");
    setup.sql("DELETE FROM extension_blobs");
    assert!(matches!(
        setup.start(),
        Err(HaexError::ExtensionNotReady { ref status }) if status == "transferring"
    ));
    assert_eq!(setup.status().0, "transferring");
}

#[test]
fn a_disabled_or_removed_extension_does_not_start() {
    let setup = installed("good-minimal.xt");
    setup.sql("UPDATE extensions SET enabled = 0");
    assert!(matches!(setup.start(), Err(HaexError::ExtensionDisabled)));
    setup.sql("UPDATE extensions SET enabled = 1, state = 'removed'");
    assert!(matches!(setup.start(), Err(HaexError::ExtensionNotFound)));
}

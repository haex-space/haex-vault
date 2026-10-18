use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::extensions::registry::install::install;
use crate::extensions::registry::remove::remove;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::VaultGate;

fn installed() -> (tempfile::TempDir, VaultDb, Uuid) {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-notes-like.xt"),
    )
    .unwrap();
    let ext = install(&vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    (dir, vault, ext)
}

#[test]
fn the_limits_start_at_the_defaults_and_keep_what_the_settings_store() {
    let (_dir, vault, ext) = installed();
    let defaults = get(&vault, ext).unwrap().values;
    assert_eq!(defaults, ExtensionLimits::from(Limits::default()));
    assert_eq!(
        defaults.out_of_bounds(),
        None,
        "the defaults lie within the bounds"
    );

    assert_eq!(defaults.model_job_timeout_ms, 300_000);
    let changed = ExtensionLimits {
        max_rows: 500,
        timeout_ms: 10_000,
        model_job_timeout_ms: 600_000,
        ..defaults
    };
    set(&vault, ext, changed).unwrap();
    assert_eq!(get(&vault, ext).unwrap().values, changed);
}

#[test]
fn a_value_outside_its_bounds_is_refused_and_nothing_changes() {
    let (_dir, vault, ext) = installed();
    let defaults = get(&vault, ext).unwrap().values;
    for limits in [
        ExtensionLimits {
            max_rows: 0,
            ..defaults
        },
        ExtensionLimits {
            max_concurrent: 101,
            ..defaults
        },
        ExtensionLimits {
            timeout_ms: 60_001,
            ..defaults
        },
        ExtensionLimits {
            model_job_timeout_ms: 9_999,
            ..defaults
        },
        ExtensionLimits {
            model_job_timeout_ms: 1_800_001,
            ..defaults
        },
    ] {
        assert!(matches!(
            set(&vault, ext, limits),
            Err(HolziError::InvalidInput { .. })
        ));
    }
    assert_eq!(get(&vault, ext).unwrap().values, defaults);
}

#[test]
fn a_removed_extension_has_no_limits_to_show_or_change() {
    let (_dir, vault, ext) = installed();
    remove(&vault, ext, false, 2).unwrap();
    assert!(matches!(
        get(&vault, ext),
        Err(HolziError::ExtensionNotFound)
    ));
    assert!(matches!(
        set(&vault, ext, ExtensionLimits::from(Limits::default())),
        Err(HolziError::ExtensionNotFound)
    ));
}

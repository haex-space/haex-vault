use std::sync::Arc;

use uuid::Uuid;

use super::commands::{write_switch, PREF_ENABLED, PREF_MUTED};
use crate::passwords::test_support::open_test_vault;
use crate::storage::preferences::{self, PrefScope};
use crate::vault_gate::VaultGate;

#[tokio::test]
async fn the_switches_are_stored_for_this_device_only() {
    let (_dir, db) = open_test_vault();
    let (here, other) = (Uuid::new_v4(), Uuid::new_v4());
    for (installation, device) in [("here", here), ("other", other)] {
        db.write(|tx| {
            tx.execute(
                "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
                 VALUES (?1, ?2, NULL, 0)",
                haex_crdt::rusqlite::params![installation, device.to_string()],
            )
        })
        .expect("device");
    }
    let db = VaultGate::new()
        .vault_db(Arc::new(db))
        .expect("a vault handle");

    write_switch(&db, here, PREF_ENABLED, true)
        .await
        .expect("stored");
    write_switch(&db, here, PREF_MUTED, true)
        .await
        .expect("stored");
    write_switch(&db, here, PREF_MUTED, false)
        .await
        .expect("stored again");

    let read = |device: Uuid, key: &'static str| {
        let db = db.clone();
        async move {
            db.read(move |r| preferences::get(r, PrefScope::Device(device), key))
                .await
                .expect("read")
        }
    };
    assert_eq!(read(here, PREF_ENABLED).await.as_deref(), Some("true"));
    assert_eq!(read(here, PREF_MUTED).await.as_deref(), Some("false"));
    assert_eq!(
        read(other, PREF_ENABLED).await,
        None,
        "other devices keep their own"
    );
}

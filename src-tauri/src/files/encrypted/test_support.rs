//! Shared setup for the tests of encrypted folders: a vault that holds content keys.

use crate::sync::content_keys::ContentKey;
use crate::vault_gate::VaultDb;

/// Stores a fresh content key of `generation` in `db`, as unwrapping its envelope would.
pub fn give_key(db: &VaultDb, generation: u64) -> ContentKey {
    let key = ContentKey::generate(generation);
    let (key_id, bytes) = (key.key_id, *key.key);
    db.write_blocking(move |tx| {
        tx.execute(
            "INSERT INTO vault_content_keys_no_sync (key_id, generation, key) VALUES (?1, ?2, ?3)",
            haex_crdt::rusqlite::params![
                key_id.as_slice(),
                i64::try_from(generation).expect("small generation"),
                bytes.as_slice()
            ],
        )?;
        Ok(())
    })
    .expect("store a content key");
    key
}

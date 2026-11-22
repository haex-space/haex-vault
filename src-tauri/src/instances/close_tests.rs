//! The vault file around a close (spec 095 T024, quickstart scenario 9, step 4): `secure_delete` is
//! on in an opened vault, so a replaced ratchet state is overwritten in the file, and the close
//! folds the write-ahead log into the file, so no earlier page stays beside it.

use std::sync::Arc;
use std::time::Duration;

use haex_crdt::rusqlite::params;

use super::fold_wal_into_file;
use crate::passwords::test_support::open_test_vault;

#[tokio::test]
async fn an_opened_vault_deletes_securely_and_the_close_leaves_an_empty_log() {
    let (dir, db) = open_test_vault();
    crate::storage::maintenance::run_after_open(&db);
    // A PRAGMA cannot run in a CRDT transaction.
    #[allow(clippy::disallowed_methods)]
    let secure_delete: i64 = db
        .with_connection(|conn| Ok(conn.query_row("PRAGMA secure_delete", [], |row| row.get(0))?))
        .expect("pragma");
    assert_eq!(secure_delete, 1);

    db.write(|tx| {
        tx.execute(
            "INSERT INTO chat_wraps_seen_no_sync (wrap_id, seen_at) VALUES (?1, ?2)",
            params!["w", 1],
        )?;
        Ok(())
    })
    .expect("write");
    let log = dir.path().join("vault.db-wal");
    let size = |path: &std::path::Path| std::fs::metadata(path).map_or(0, |meta| meta.len());
    assert!(size(&log) > 0, "the write is in the log first");

    fold_wal_into_file(&Arc::new(db), Duration::from_secs(10)).await;
    assert_eq!(size(&log), 0, "the close folded it into the file");
}

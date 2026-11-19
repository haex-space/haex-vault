//! Migration tests for `0066_chat_record_relays` (spec 095 T013): the account row keeps its values
//! and starts with no relays, the table stays outside the sync.

// These tests read `pragma_*` directly and seed rows below the CRDT write path.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::migrations_extensions_tests::has_hlc_column;
use super::migrations_tests::{column_names, migration_source_before, open};
use crate::identity::haex_migration_source;

#[tokio::test]
async fn migration_0066_keeps_the_account_and_starts_without_relays() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0066_chat_record_relays");
        let db = open(dir.path(), "record-relays", true, before);
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO chat_ratchet_account_no_sync \
                 (installation_uuid, account, start_key_made_at, record_published_at) \
                 VALUES ('i1', '{}', 5, 6)",
                [],
            )?;
            Ok(())
        })
        .expect("seed");
        drop(db);

        let db = open(dir.path(), "record-relays", false, haex_migration_source());
        assert!(column_names(&db, "chat_ratchet_account_no_sync")
            .iter()
            .any(|name| name == "record_relays"));
        assert!(!has_hlc_column(&db, "chat_ratchet_account_no_sync"));
        let row: (i64, Option<i64>, String) = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT start_key_made_at, record_published_at, record_relays \
                     FROM chat_ratchet_account_no_sync WHERE installation_uuid = ?1",
                    params!["i1"],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?)
            })
            .expect("read");
        assert_eq!(row, (5, Some(6), "[]".to_owned()));
    })
    .await
    .expect("join");
}

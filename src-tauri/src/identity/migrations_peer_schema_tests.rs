//! Migration tests for `0040_device_peer_schema` (spec 059, data-model.md): a fresh vault and a
//! vault from the schema before end with the three device-local columns, empty for a device whose
//! state was never seen, and the presence row they extend keeps its values.

// These tests read `pragma_*` and write a presence row directly, which the CRDT write path does
// not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::migrations_tests::{migration_source_before, open};
use crate::identity::holzi_migration_source;

const COLUMNS: [&str; 3] = ["peer_protocol", "peer_migration", "peer_trigger"];

fn columns(db: &Database) -> Vec<String> {
    db.with_connection(|conn| {
        let mut stmt =
            conn.prepare("SELECT name FROM pragma_table_info('device_presence_no_sync')")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .expect("read columns")
}

#[tokio::test]
async fn migration_0040_gives_a_fresh_vault_the_peer_schema_columns() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(
            dir.path(),
            "peer-schema-fresh",
            true,
            holzi_migration_source(),
        );
        let present = columns(&db);
        for column in COLUMNS {
            assert!(present.iter().any(|c| c == column), "{column} exists");
        }
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0040_keeps_a_presence_row_and_leaves_the_state_unknown() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = open(
            dir.path(),
            "peer-schema-upgrade",
            true,
            migration_source_before("0040_device_peer_schema"),
        );
        for column in COLUMNS {
            assert!(!columns(&old).iter().any(|c| c == column), "{column} is new");
        }
        old.with_connection(|conn| {
            conn.execute(
                "INSERT INTO device_presence_no_sync (device_pubkey, last_seen, endpoint_addr, problem) \
                 VALUES (?1, 42, NULL, 'incompatible_version')",
                params![[7u8; 32].as_slice()],
            )?;
            Ok(())
        })
        .expect("insert");
        drop(old);

        let db = open(
            dir.path(),
            "peer-schema-upgrade",
            false,
            holzi_migration_source(),
        );
        let row: (i64, Option<String>, Option<i64>, Option<i64>, Option<i64>) = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT last_seen, problem, peer_protocol, peer_migration, peer_trigger \
                     FROM device_presence_no_sync WHERE device_pubkey = ?1",
                    params![[7u8; 32].as_slice()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )?)
            })
            .expect("read row");
        assert_eq!(
            row,
            (42, Some("incompatible_version".to_owned()), None, None, None)
        );
    })
    .await
    .expect("join");
}

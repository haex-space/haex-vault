//! Migration tests for `0067_chat_attachment_inline` (spec 072 FR-012): a fresh vault and a vault
//! from the schema before end with `chat_attachments.inline`, and every attachment from before is an
//! ordinary file.

// These tests read `pragma_*` directly and seed rows below the CRDT write path.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::migrations_extensions_tests::has_hlc_column;
use super::migrations_tests::{column_names, migration_source_before, open};
use crate::identity::haex_migration_source;

#[tokio::test]
async fn migration_0067_gives_a_fresh_vault_the_inline_column() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = open(dir.path(), "inline-fresh", true, haex_migration_source());
        assert!(column_names(&db, "chat_attachments")
            .iter()
            .any(|name| name == "inline"));
        assert!(
            has_hlc_column(&db, "chat_attachments"),
            "chat_attachments stays tracked by haex-crdt"
        );
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn migration_0067_keeps_attachments_as_ordinary_files() {
    tokio::task::spawn_blocking(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = migration_source_before("0067_chat_attachment_inline");
        let db = open(dir.path(), "inline-upgrade", true, before);
        db.with_connection(|conn| {
            conn.execute(
                "INSERT INTO chat_attachments (id, message_id, kind, name) \
                 VALUES ('a1', 'm1', 'file', 'Plan.pdf')",
                [],
            )?;
            Ok(())
        })
        .expect("seed");
        drop(db);

        let db = open(dir.path(), "inline-upgrade", false, haex_migration_source());
        let (name, inline): (String, i64) = db
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT name, inline FROM chat_attachments WHERE id = ?1",
                    params!["a1"],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .expect("read");
        assert_eq!((name.as_str(), inline), ("Plan.pdf", 0));
    })
    .await
    .expect("join");
}

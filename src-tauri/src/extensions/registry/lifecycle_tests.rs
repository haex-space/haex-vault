//! The life cycle of an extension across devices (spec 017, T076, research R11): two or three
//! devices on the real pull path of `sync::test_support`, each following the registry with
//! [`reconcile`].

// The tests read and change rows directly to observe and disturb each device.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use haex_crdt::rusqlite::params;

use super::test_support::*;
use super::*;
use crate::extensions::permissions::store::{self as permission_store, NewPermission};
use crate::extensions::permissions::{PermissionKind, PermissionStatus, VAULT_WIDE};
use crate::extensions::registry::remove::remove;
use crate::storage::query;

#[test]
fn an_install_on_a_reaches_b_which_verifies_and_gets_the_data() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('i1', 'first')");

    b.pull(&a);
    let changed = b.follow();
    assert!(ready(&b, ext));
    assert!(changed.iter().any(|c| c.status == "ready" && !c.reload));
    assert_eq!(b.count("SELECT COUNT(*) FROM `{t}`"), Some(1));
    assert_eq!(b.parked(), 0);
}

#[test]
fn a_bundle_corrupted_on_b_fails_there_while_other_data_keeps_syncing() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    a.follow();
    b.pull(&a);
    b.write("UPDATE extension_blobs SET data = x'00'");

    b.follow();
    assert_eq!(
        b.status(ext).map(|(s, _)| s).as_deref(),
        Some("signature_failed")
    );
    a.write("INSERT INTO chat_threads (id, title, created_at, updated_at) VALUES ('t', 't', 1, 1)");
    b.pull(&a);
    assert_eq!(b.count("SELECT COUNT(*) FROM chat_threads"), Some(1));
}

#[test]
fn an_update_on_a_applies_the_new_migrations_on_b_and_reloads_its_tabs() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    a.follow();
    b.pull(&a);
    b.follow();
    let first = b.status(ext).and_then(|(_, bundle)| bundle);

    a.install(&bundle("1.1.0", &[INIT, TAG]));
    a.follow();
    a.write("INSERT INTO `{t}` (id, label, tag) VALUES ('i1', 'first', 'red')");
    b.pull(&a);
    let changed = b.follow();

    let second = b.status(ext).and_then(|(_, bundle)| bundle);
    assert_ne!(first, second, "B switched to the new bundle");
    assert!(changed.iter().any(|c| c.reload), "open tabs reload");
    assert_eq!(
        b.count("SELECT COUNT(*) FROM `{t}` WHERE tag = 'red'"),
        Some(1)
    );
}

#[test]
fn concurrent_installs_of_two_versions_end_on_the_higher_one_everywhere() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.2.0", &[INIT]));
    b.install(&bundle("1.3.0", &[INIT]));
    a.pull(&b);
    b.pull(&a);
    a.follow();
    b.follow();
    let on_a = a.status(ext).and_then(|(_, bundle)| bundle);
    let on_b = b.status(ext).and_then(|(_, bundle)| bundle);
    assert!(on_a.is_some());
    assert_eq!(on_a, on_b);
    let version: String = query::read(a.device.db(), |r| {
        r.query_row(
            "SELECT version FROM extension_bundles WHERE id = ?1",
            params![on_a.clone().unwrap()],
            |row| row.get(0),
        )
    })
    .expect("version")
    .expect("bundle");
    assert_eq!(version, "1.3.0");
}

#[test]
fn a_shell_permission_from_a_does_not_hold_on_b_a_file_permission_does() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    let me = a.me;
    a.vault
        .write_blocking(move |tx| {
            for (kind, action, target) in [
                (PermissionKind::Shell, "execute", "/usr/bin/git"),
                (PermissionKind::Filesystem, "read", "/tmp/notes"),
            ] {
                permission_store::put(
                    tx,
                    ext,
                    &NewPermission {
                        kind: kind.as_str(),
                        action,
                        target,
                        status: "granted",
                        declared: false,
                        vault_device_uuid: kind.scope_on(me),
                    },
                    1,
                )?;
            }
            Ok(())
        })
        .expect("grant");
    b.pull(&a);
    let on = |node: &Node, kind: PermissionKind| {
        let me = node.me;
        node.vault
            .read_blocking(move |q| {
                permission_store::candidates(q, ext, kind, me).map_err(Into::into)
            })
            .expect("candidates")
            .len()
    };
    assert_eq!(on(&a, PermissionKind::Shell), 1);
    assert_eq!(on(&b, PermissionKind::Shell), 0, "the shell stays on A");
    assert_eq!(on(&a, PermissionKind::Filesystem), 1);
    assert_eq!(on(&b, PermissionKind::Filesystem), 1, "files hold on B too");
}

#[test]
fn an_old_shell_allow_for_every_device_counts_nowhere_an_old_deny_still_holds() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    // Rows as haex-vault wrote them before the clarification of 2026-10-06: the shell vault-wide.
    a.vault
        .write_blocking(move |tx| {
            for (target, status) in [("/usr/bin/git", "granted"), ("/usr/bin/rm", "denied")] {
                permission_store::put(
                    tx,
                    ext,
                    &NewPermission {
                        kind: PermissionKind::Shell.as_str(),
                        action: "execute",
                        target,
                        status,
                        declared: false,
                        vault_device_uuid: VAULT_WIDE,
                    },
                    1,
                )?;
            }
            Ok(())
        })
        .expect("old rows");
    b.pull(&a);
    for node in [&a, &b] {
        let me = node.me;
        let held = node
            .vault
            .read_blocking(move |q| {
                permission_store::candidates(q, ext, PermissionKind::Shell, me).map_err(Into::into)
            })
            .expect("candidates");
        assert_eq!(held.len(), 1, "only the deny holds");
        assert_eq!(held[0].status, PermissionStatus::Denied);
    }
}

#[test]
fn delete_data_drops_the_tables_on_b_and_late_groups_from_before_are_discarded() {
    let (a, b, c) = (Node::new(), Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('i1', 'first')");
    b.pull(&a);
    b.follow();
    c.pull(&a);
    c.follow();
    // Written on C before the removal, reaching B only after it.
    c.write("INSERT INTO `{t}` (id, label) VALUES ('late', 'from c')");

    remove(&a.vault, ext, true, now()).expect("remove");
    a.follow();
    assert!(!a.has_items(), "the removing device clears up too");
    b.pull(&a);
    b.follow();
    assert!(!b.has_items());
    assert_eq!(b.journal(), 0);

    b.pull(&c);
    assert!(!b.has_items());
    assert_eq!(b.parked(), 0, "older than the purge: dropped, not parked");
}

#[test]
fn a_reinstall_after_removal_starts_fresh_with_newer_data() {
    let (a, b) = (Node::new(), Node::new());
    let v1 = bundle("1.0.0", &[INIT]);
    let ext = a.install(&v1);
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('old', 'before')");
    b.pull(&a);
    b.follow();
    remove(&a.vault, ext, true, now()).expect("remove");
    a.follow();
    b.pull(&a);
    b.follow();

    a.install(&v1);
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('new', 'after')");
    b.pull(&a);
    b.follow();
    assert!(ready(&b, ext));
    assert_eq!(
        b.count("SELECT COUNT(*) FROM `{t}` WHERE id = 'new'"),
        Some(1)
    );
    assert_eq!(
        b.count("SELECT COUNT(*) FROM `{t}` WHERE id = 'old'"),
        Some(0)
    );
}

#[test]
fn a_device_offline_during_removal_and_reinstall_still_clears_up() {
    let (a, c) = (Node::new(), Node::new());
    let v1 = bundle("1.0.0", &[INIT]);
    let ext = a.install(&v1);
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('old', 'before')");
    c.pull(&a);
    c.follow();

    // C is offline while A removes with "delete data" and installs again.
    remove(&a.vault, ext, true, now()).expect("remove");
    a.follow();
    a.install(&v1);
    a.follow();

    c.pull(&a);
    c.follow();
    assert!(ready(&c, ext), "C sees state = installed only");
    assert_eq!(
        c.count("SELECT COUNT(*) FROM `{t}` WHERE id = 'old'"),
        Some(0)
    );
}

#[test]
fn rows_written_after_a_reinstall_survive_the_clear_up_of_a_device_that_was_offline() {
    let (a, c) = (Node::new(), Node::new());
    let v1 = bundle("1.0.0", &[INIT]);
    let ext = a.install(&v1);
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('old', 'before')");
    c.pull(&a);
    c.follow();

    // C is offline while A removes with "delete data", installs again and writes; C gets all of
    // it in one pull, before it clears up.
    remove(&a.vault, ext, true, now()).expect("remove");
    a.follow();
    a.install(&v1);
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('new', 'after')");

    c.pull(&a);
    c.follow();
    assert!(ready(&c, ext));
    assert_eq!(
        c.count("SELECT COUNT(*) FROM `{t}` WHERE id = 'old'"),
        Some(0)
    );
    assert_eq!(
        c.count("SELECT COUNT(*) FROM `{t}` WHERE id = 'new'"),
        Some(1),
        "the clear-up drops only what is older than the removal"
    );
}

#[test]
fn keep_data_keeps_tables_journal_and_parked_groups_and_a_reinstall_does_not_migrate_again() {
    let (a, b) = (Node::new(), Node::new());
    let v1 = bundle("1.0.0", &[INIT]);
    let ext = a.install(&v1);
    a.follow();
    a.write("INSERT INTO `{t}` (id, label) VALUES ('i1', 'kept')");
    b.pull(&a);
    b.follow();
    // A group for a table of the extension B does not have, waiting.
    let waiting = serde_json::json!([{
        "tableName": format!("{}__tasks__later", public_key()),
        "rowPks": "{\"id\":\"x\"}",
        "columnName": "label",
        "hlcTimestamp": "h",
        "value": "v",
        "deviceId": "d",
    }])
    .to_string();
    b.device
        .db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO sync_parked_groups_no_sync (origin, hlc, extension_prefix, tables, \
                 group_blob, bytes, reason, parked_at) VALUES ('o', 'h', ?1, '[]', ?2, 2, \
                 'missing_table', 1)",
                params![format!("{}__tasks__", public_key()), waiting.into_bytes()],
            )
            .map(drop)
        })
        .expect("park");

    remove(&a.vault, ext, false, now()).expect("remove");
    b.pull(&a);
    b.follow();
    assert!(b.has_items());
    assert_eq!(b.count("SELECT COUNT(*) FROM `{t}`"), Some(1));
    assert_eq!(b.journal(), 1);
    assert_eq!(b.parked(), 1);

    a.install(&v1);
    a.follow();
    b.pull(&a);
    b.follow();
    assert!(ready(&b, ext));
    assert_eq!(b.journal(), 1, "no migration ran again");
    assert_eq!(b.count("SELECT COUNT(*) FROM `{t}`"), Some(1));
}

/// Writes through the extension's own SQL path, as `extension_database_execute` does.
fn write_as_extension(node: &Node, sql: &str) {
    use crate::extensions::ids::{ExtensionName, PublicKey};
    use crate::extensions::sql::exec::{existing_tables, prepare, run, Limits};
    use crate::extensions::sql::policy::SqlPolicy;
    let own = TablePrefix {
        public_key: PublicKey::parse(&public_key()).expect("key"),
        name: ExtensionName::parse("tasks").expect("name"),
    };
    let policy = Arc::new(SqlPolicy::own_only(own, node.me));
    let limits = Limits::default();
    let existing = existing_tables(&node.vault).expect("tables");
    let checked = prepare(
        &sql.replace("{t}", &items()),
        Vec::new(),
        &policy,
        &limits,
        &existing,
    )
    .expect("prepare");
    run(&node.vault, policy, &limits, vec![checked]).expect("run");
}

#[test]
fn rows_an_extension_writes_on_b_reach_a() {
    let (a, b) = (Node::new(), Node::new());
    let ext = a.install(&bundle("1.0.0", &[INIT]));
    a.follow();
    b.pull(&a);
    b.follow();
    assert!(ready(&b, ext));

    write_as_extension(&b, "INSERT INTO `{t}` (id, label) VALUES ('b1', 'from b')");
    a.pull(&b);
    assert_eq!(
        a.count("SELECT COUNT(*) FROM `{t}` WHERE id = 'b1'"),
        Some(1)
    );
}

//! The stored file permissions of agents (data-model.md): rows written, read back as grants,
//! derived ids, the field rules, and the rows of a storage leaving with it.

// These tests count rows directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use super::{
    folder_target, grants_of, list, permission_id, remove, remove_storage, set, AgentFileKind,
    AgentFilePermission, AgentFileReach, AgentFileStatus, BUILTIN_AGENT,
};
use crate::files::access::{AgentGrants, DeviceGrant, FolderGrant, Locality, StorageGrant};
use crate::remote_storage::test_support::vault;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

fn row(
    agent: &str,
    kind: AgentFileKind,
    target: &str,
    status: AgentFileStatus,
) -> AgentFilePermission {
    AgentFilePermission {
        agent_id: agent.to_owned(),
        kind,
        target: target.to_owned(),
        status,
        updated_at: 1,
        reach: None,
    }
}

fn put(db: &VaultDb, row: AgentFilePermission) -> crate::error::Result<()> {
    db.write_blocking(move |tx| Ok(set(tx, &row)?))
}

fn grants(db: &VaultDb, agent: &str) -> AgentGrants {
    let agent = agent.to_owned();
    db.read_blocking(move |q| Ok(grants_of(q, &agent)?))
        .expect("grants")
}

#[test]
fn no_row_is_no_grant() {
    let (_dir, db) = vault();
    assert_eq!(grants(&db, BUILTIN_AGENT), AgentGrants::default());
}

#[test]
fn rows_become_the_grants_of_their_agent() {
    let (_dir, db) = vault();
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Device,
            "",
            AgentFileStatus::Denied,
        ),
    )
    .expect("put");
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Storage,
            "s1",
            AgentFileStatus::Read,
        ),
    )
    .expect("put");
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Storage,
            "s2",
            AgentFileStatus::ReadWrite,
        ),
    )
    .expect("put");
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Storage,
            "s3",
            AgentFileStatus::Denied,
        ),
    )
    .expect("put");
    put(
        &db,
        row(
            "agent-x",
            AgentFileKind::Storage,
            "s1",
            AgentFileStatus::ReadWrite,
        ),
    )
    .expect("put");

    let builtin = grants(&db, BUILTIN_AGENT);
    assert_eq!(builtin.device, Some(DeviceGrant::Denied));
    assert_eq!(builtin.storages.get("s1"), Some(&StorageGrant::Read));
    assert_eq!(builtin.storages.get("s2"), Some(&StorageGrant::ReadWrite));
    assert_eq!(builtin.storages.get("s3"), Some(&StorageGrant::Denied));
    assert_eq!(
        grants(&db, "agent-x").storages.get("s1"),
        Some(&StorageGrant::ReadWrite)
    );
    assert_eq!(db.read_blocking(|q| Ok(list(q)?)).expect("list").len(), 5);
}

#[test]
fn a_new_answer_updates_the_one_row() {
    let (_dir, db) = vault();
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Storage,
            "s1",
            AgentFileStatus::Read,
        ),
    )
    .expect("put");
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Storage,
            "s1",
            AgentFileStatus::ReadWrite,
        ),
    )
    .expect("put");
    let rows = db.read_blocking(|q| Ok(list(q)?)).expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, AgentFileStatus::ReadWrite);
    let id = permission_id(BUILTIN_AGENT, AgentFileKind::Storage, "s1");
    let stored = db
        .read_blocking(move |q| {
            q.query_row("SELECT id FROM agent_file_permissions", &[], |r| {
                r.get::<_, String>(0)
            })
        })
        .expect("id");
    assert_eq!(stored, Some(id));
}

#[test]
fn ids_are_derived_and_differ_per_agent_kind_and_target() {
    let a = permission_id(BUILTIN_AGENT, AgentFileKind::Storage, "s1");
    assert_eq!(
        a,
        permission_id(BUILTIN_AGENT, AgentFileKind::Storage, "s1")
    );
    assert_ne!(a, permission_id("agent-x", AgentFileKind::Storage, "s1"));
    assert_ne!(
        a,
        permission_id(BUILTIN_AGENT, AgentFileKind::Storage, "s2")
    );
    assert_ne!(
        permission_id(BUILTIN_AGENT, AgentFileKind::Device, ""),
        permission_id(BUILTIN_AGENT, AgentFileKind::Storage, "")
    );
}

#[test]
fn a_status_or_target_that_does_not_fit_the_kind_is_refused() {
    let (_dir, db) = vault();
    for bad in [
        row(
            BUILTIN_AGENT,
            AgentFileKind::Device,
            "",
            AgentFileStatus::Read,
        ),
        row(
            BUILTIN_AGENT,
            AgentFileKind::Device,
            "s1",
            AgentFileStatus::Granted,
        ),
        row(
            BUILTIN_AGENT,
            AgentFileKind::Storage,
            "s1",
            AgentFileStatus::Granted,
        ),
        row(
            BUILTIN_AGENT,
            AgentFileKind::Storage,
            "",
            AgentFileStatus::Read,
        ),
        row("", AgentFileKind::Device, "", AgentFileStatus::Granted),
    ] {
        assert!(put(&db, bad.clone()).is_err(), "{bad:?}");
    }
    assert!(db.read_blocking(|q| Ok(list(q)?)).expect("list").is_empty());
}

#[test]
fn removing_a_storage_takes_its_permissions_along() {
    let (_dir, db) = vault();
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Storage,
            "s1",
            AgentFileStatus::Read,
        ),
    )
    .expect("put");
    put(
        &db,
        row(
            "agent-x",
            AgentFileKind::Storage,
            "s1",
            AgentFileStatus::Denied,
        ),
    )
    .expect("put");
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Storage,
            "s2",
            AgentFileStatus::Read,
        ),
    )
    .expect("put");
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Device,
            "",
            AgentFileStatus::Granted,
        ),
    )
    .expect("put");
    db.write_blocking(|tx| Ok(crate::remote_storage::store::remove_storage(tx, "s1")?))
        .expect("remove");
    let rows = db.read_blocking(|q| Ok(list(q)?)).expect("list");
    let targets: Vec<&str> = rows.iter().map(|r| r.target.as_str()).collect();
    assert_eq!(targets, ["", "s2"]);
}

#[test]
fn a_removed_row_brings_back_the_default() {
    let (_dir, db) = vault();
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Storage,
            "s1",
            AgentFileStatus::Read,
        ),
    )
    .expect("put");
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::Device,
            "",
            AgentFileStatus::Denied,
        ),
    )
    .expect("put");
    db.write_blocking(|tx| Ok(remove(tx, BUILTIN_AGENT, AgentFileKind::Device, "")?))
        .expect("remove");
    let grants = grants(&db, BUILTIN_AGENT);
    assert_eq!(grants.device, None);
    assert_eq!(grants.storages.get("s1"), Some(&StorageGrant::Read));
}

#[test]
fn a_folder_row_keeps_its_reach_and_leaves_with_its_storage() {
    let (_dir, db) = vault();
    let folder = |storage: &str, reach: AgentFileReach| AgentFilePermission {
        reach: Some(reach),
        ..row(
            BUILTIN_AGENT,
            AgentFileKind::EncryptedFolder,
            &folder_target(storage, "abc"),
            AgentFileStatus::Read,
        )
    };
    put(&db, folder("s1", AgentFileReach::Cloud)).expect("set");
    put(&db, folder("s10", AgentFileReach::Local)).expect("set");
    let held = grants(&db, BUILTIN_AGENT);
    assert_eq!(
        held.folders.get("s1/abc"),
        Some(&FolderGrant {
            grant: StorageGrant::Read,
            reach: Locality::Cloud
        })
    );
    // A folder needs its reach and a target of storage and folder; the others carry none.
    for wrong in [
        AgentFilePermission {
            reach: None,
            ..folder("s1", AgentFileReach::Local)
        },
        AgentFilePermission {
            target: "s1".to_owned(),
            ..folder("s1", AgentFileReach::Local)
        },
        AgentFilePermission {
            reach: Some(AgentFileReach::Local),
            ..row(
                BUILTIN_AGENT,
                AgentFileKind::Storage,
                "s1",
                AgentFileStatus::Read,
            )
        },
    ] {
        assert!(put(&db, wrong).is_err());
    }
    // Removing storage s1 takes its folder along, not the one of s10.
    db.write_blocking(|tx| Ok(remove_storage(tx, "s1")?))
        .expect("remove");
    let left: Vec<String> = db
        .read_blocking(|q| Ok(list(q)?))
        .expect("list")
        .into_iter()
        .map(|row| row.target)
        .collect();
    assert_eq!(left, ["s10/abc"]);
}

#[test]
fn managing_file_shares_is_off_until_granted_and_takes_no_target() {
    let (_dir, db) = vault();
    assert!(!grants(&db, BUILTIN_AGENT).file_shares, "off by default");
    for bad in [
        row(
            BUILTIN_AGENT,
            AgentFileKind::FileShares,
            "",
            AgentFileStatus::Read,
        ),
        row(
            BUILTIN_AGENT,
            AgentFileKind::FileShares,
            "x",
            AgentFileStatus::Granted,
        ),
    ] {
        assert!(put(&db, bad.clone()).is_err(), "{bad:?}");
    }
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::FileShares,
            "",
            AgentFileStatus::Granted,
        ),
    )
    .expect("granted");
    assert!(grants(&db, BUILTIN_AGENT).file_shares);
    assert!(!grants(&db, "other").file_shares, "per agent");
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::FileShares,
            "",
            AgentFileStatus::Denied,
        ),
    )
    .expect("denied");
    assert!(!grants(&db, BUILTIN_AGENT).file_shares);
}

#[test]
fn managing_sync_rules_is_off_until_granted_and_takes_no_target() {
    let (_dir, db) = vault();
    assert!(!grants(&db, BUILTIN_AGENT).sync_rules, "off by default");
    for bad in [
        row(
            BUILTIN_AGENT,
            AgentFileKind::SyncRules,
            "",
            AgentFileStatus::ReadWrite,
        ),
        row(
            BUILTIN_AGENT,
            AgentFileKind::SyncRules,
            "x",
            AgentFileStatus::Granted,
        ),
    ] {
        assert!(put(&db, bad.clone()).is_err(), "{bad:?}");
    }
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::SyncRules,
            "",
            AgentFileStatus::Granted,
        ),
    )
    .expect("granted");
    let granted = grants(&db, BUILTIN_AGENT);
    assert!(granted.sync_rules);
    assert!(!granted.file_shares, "a permission of its own");
    assert!(!grants(&db, "other").sync_rules, "per agent");
    let stored: Vec<AgentFileKind> = db
        .read_blocking(|q| Ok(list(q)?))
        .expect("list")
        .into_iter()
        .map(|row| row.kind)
        .collect();
    assert_eq!(stored, [AgentFileKind::SyncRules], "read back as itself");
    put(
        &db,
        row(
            BUILTIN_AGENT,
            AgentFileKind::SyncRules,
            "",
            AgentFileStatus::Denied,
        ),
    )
    .expect("denied");
    assert!(!grants(&db, BUILTIN_AGENT).sync_rules);
}

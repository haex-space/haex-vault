//! The memory in the resolver (spec 097 data-model.md "Rights", FR-006, T025): "Standard" reads and
//! writes from the factory, also when it was edited before the group existed; every other profile
//! without the group has it off; a stored level counts as stored.

// "Standard" from before spec 097 is seeded below the store: its rows lack the group `memory`.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::resolve::profile_rights;
use super::store;
use super::test_support::vault;
use super::STANDARD_PROFILE_ID;
use crate::mcp_server::grants::{MemoryLevel, OperateLevel, Rights};

fn reading_with(memory: MemoryLevel) -> Rights {
    Rights {
        operate: [("wm.read".to_owned(), OperateLevel::Read)]
            .into_iter()
            .collect(),
        memory,
        ..Rights::default()
    }
}

#[tokio::test]
async fn standard_reads_and_writes_from_the_factory() {
    let v = vault();
    let rights =
        v.db.read(|q| profile_rights(q, STANDARD_PROFILE_ID))
            .await
            .expect("read");
    assert_eq!(rights.memory, MemoryLevel::ReadWrite);
}

#[tokio::test]
async fn standard_edited_before_the_memory_keeps_the_factory_level() {
    let v = vault();
    v.db.write(|tx| {
        store::set_rights(tx, STANDARD_PROFILE_ID, &reading_with(MemoryLevel::Off))?;
        tx.execute(
            "DELETE FROM haex_agent_profile_grants WHERE profile_id = ?1 AND grant_group = 'memory'",
            params![STANDARD_PROFILE_ID.to_string()],
        )?;
        Ok(())
    })
    .await
    .expect("seed rows without the memory");
    let rights =
        v.db.read(|q| profile_rights(q, STANDARD_PROFILE_ID))
            .await
            .expect("read");
    assert_eq!(rights.memory, MemoryLevel::ReadWrite);
    assert_eq!(
        rights.operate.get("wm.read"),
        Some(&OperateLevel::Read),
        "the other rows still count"
    );
}

#[tokio::test]
async fn a_stored_level_counts_in_every_profile() {
    let v = vault();
    let other = Uuid::new_v4();
    v.db.write(move |tx| {
        store::set_rights(tx, STANDARD_PROFILE_ID, &reading_with(MemoryLevel::Off))?;
        store::create_profile(tx, other, "Ohne Gedächtnis").map_err(haex_crdt::Error::from)?;
        store::set_rights(tx, other, &reading_with(MemoryLevel::Read))?;
        Ok(())
    })
    .await
    .expect("write");
    let (standard, read) =
        v.db.read(move |q| {
            Ok((
                profile_rights(q, STANDARD_PROFILE_ID)?,
                profile_rights(q, other)?,
            ))
        })
        .await
        .expect("read");
    assert_eq!(
        standard.memory,
        MemoryLevel::Off,
        "switched off by the user"
    );
    assert_eq!(read.memory, MemoryLevel::Read);
}

#[tokio::test]
async fn another_profile_without_the_group_has_it_off() {
    let v = vault();
    let other = Uuid::new_v4();
    v.db.write(move |tx| {
        store::create_profile(tx, other, "Neu").map_err(haex_crdt::Error::from)?;
        Ok(())
    })
    .await
    .expect("write");
    let rights =
        v.db.read(move |q| profile_rights(q, other))
            .await
            .expect("read");
    assert_eq!(rights.memory, MemoryLevel::Off);
}

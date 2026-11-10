//! Tests for what the user changes about an agent (spec 021 US7, T059, T060) against a real
//! temporary vault.

use std::sync::Arc;

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::grants::{OperateLevel, Rights};
use super::manage::{
    expire, is_expired, list_with_rights, renew, revoke, unblock, Renewal, EXPIRES_AFTER_MILLIS,
};
use super::registry::{
    block, get, register, AgentKind, AgentMode, AgentStatus, NewAgent, Transport,
};
use crate::agent_profiles::STANDARD_PROFILE_ID;
use crate::identity::{installation_id_path, read_or_mint_installation_uuid};
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::storage::query::Query;
use crate::vault_gate::{VaultDb, VaultGate};

const NOW: i64 = 1_800_000_000_000;

struct Vault {
    _dir: tempfile::TempDir,
    db: VaultDb,
    device: Uuid,
}

fn vault() -> Vault {
    let (dir, database) = open_test_vault();
    let installation =
        read_or_mint_installation_uuid(&installation_id_path(dir.path())).expect("installation");
    let device = crate::storage::query::read(&database, |q| {
        known_devices::get_vault_device_uuid(q, installation)
    })
    .expect("read")
    .expect("this device");
    let db = VaultGate::new()
        .vault_db(Arc::new(database))
        .expect("a vault handle");
    Vault {
        _dir: dir,
        db,
        device,
    }
}

fn codex() -> NewAgent {
    NewAgent {
        kind: AgentKind::External,
        slug: "codex".to_owned(),
        client_name: "codex".to_owned(),
        client_version: None,
        transport: Transport::Ipc,
    }
}

async fn registered(v: &Vault, at: i64) -> Uuid {
    let device = v.device;
    v.db.write(move |tx| register(tx, device, &codex(), "hash", STANDARD_PROFILE_ID, at))
        .await
        .expect("register")
        .id
}

#[tokio::test]
async fn a_registration_expires_after_ninety_days_without_a_connection() {
    let v = vault();
    let id = registered(&v, NOW).await;
    let device = v.device;
    let agent =
        v.db.read(move |q| get(q, device, id))
            .await
            .expect("read")
            .expect("row");
    assert!(!is_expired(&agent, NOW + EXPIRES_AFTER_MILLIS));
    assert!(is_expired(&agent, NOW + EXPIRES_AFTER_MILLIS + 1));

    v.db.write(move |tx| expire(tx, device, id))
        .await
        .expect("expire");
    let expired =
        v.db.read(move |q| get(q, device, id))
            .await
            .expect("read")
            .expect("row");
    assert_eq!(expired.status, AgentStatus::Expired);
    assert!(is_expired(&expired, NOW));

    let chosen = Rights {
        operate: [("wm.read".to_owned(), OperateLevel::Write)]
            .into_iter()
            .collect(),
        browser: None,
        ..Default::default()
    };
    let renewal = Renewal {
        client_name: "codex".to_owned(),
        client_version: Some("2".to_owned()),
    };
    let picked = Uuid::new_v4();
    let stored = chosen.clone();
    v.db.write(move |tx| {
        crate::agent_profiles::store::create_profile(tx, picked, "Chosen")?;
        crate::agent_profiles::store::set_rights(tx, picked, &stored)?;
        crate::agent_profiles::store::set_active(tx, picked)
    })
    .await
    .expect("profile");
    let renewed =
        v.db.write(move |tx| renew(tx, device, id, &renewal, "new-hash", picked, NOW + 5))
            .await
            .expect("renew")
            .expect("row");
    assert_eq!(renewed.status, AgentStatus::Registered);
    assert_eq!(renewed.mode, AgentMode::Auto);
    assert_eq!(renewed.last_seen_at, Some(NOW + 5));
    let listed =
        v.db.read(move |q| list_with_rights(q, device))
            .await
            .expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].1, chosen);
    assert_eq!(listed[0].2 .0, picked);
}

#[tokio::test]
async fn a_revoked_expired_registration_cannot_be_renewed() {
    let v = vault();
    let id = registered(&v, NOW).await;
    let device = v.device;
    let renewed =
        v.db.write(move |tx| {
            expire(tx, device, id)?;
            revoke(tx, device, id)?;
            renew(
                tx,
                device,
                id,
                &Renewal {
                    client_name: "codex".to_owned(),
                    client_version: None,
                },
                "new-hash",
                STANDARD_PROFILE_ID,
                NOW + 1,
            )
        })
        .await
        .expect("revoked registrations do not renew as a storage error");
    assert_eq!(renewed, None);
}

#[tokio::test]
async fn revoking_forgets_the_agent_and_its_memberships() {
    let v = vault();
    let id = registered(&v, NOW).await;
    let other = registered(&v, NOW).await;
    let device = v.device;
    v.db.write(move |tx| {
        let profile = Uuid::new_v4();
        crate::agent_profiles::store::create_profile(tx, profile, "Shared")?;
        for agent in [id, other] {
            crate::agent_profiles::store::add_member(
                tx,
                &crate::agent_profiles::Subject::Agent(agent),
                profile,
            )?;
        }
        Ok(())
    })
    .await
    .expect("memberships");

    assert!(v
        .db
        .write(move |tx| revoke(tx, device, id))
        .await
        .expect("revoke"));
    let counts =
        v.db.read(move |q| {
            let count = |q: &mut _, sql: &str, agent: Uuid| -> haex_crdt::Result<i64> {
                Ok(Query::query_row(q, sql, params![agent.to_string()], |row| {
                    row.get::<_, i64>(0)
                })?
                .unwrap_or(0))
            };
            Ok([
                count(q, "SELECT COUNT(*) FROM haex_mcp_agents WHERE id = ?1", id)?,
                count(
                    q,
                    "SELECT COUNT(*) FROM haex_agent_profile_members WHERE subject = 'agent:' || ?1",
                    id,
                )?,
                count(
                    q,
                    "SELECT COUNT(*) FROM haex_agent_profile_members WHERE subject = 'agent:' || ?1",
                    other,
                )?,
            ])
        })
        .await
        .expect("counts");
    assert_eq!(counts, [0, 0, 1], "another agent keeps its own");
    assert!(!v
        .db
        .write(move |tx| revoke(tx, device, id))
        .await
        .expect("again"));
}

#[tokio::test]
async fn revoking_from_another_device_keeps_the_agents_data() {
    let v = vault();
    let (device, agent, profile) = (v.device, registered(&v, NOW).await, Uuid::new_v4());
    let subject = crate::agent_profiles::Subject::Agent(agent);
    let s = subject.clone();
    v.db.write(move |tx| {
        crate::agent_profiles::store::create_profile(tx, profile, "Shared")?;
        crate::agent_profiles::store::add_member(tx, &s, profile)
    })
    .await
    .expect("profile assignment");
    assert!(!v
        .db
        .write(move |tx| revoke(tx, Uuid::new_v4(), agent))
        .await
        .expect("revoke from unrelated device"));
    let (member, still_registered) =
        v.db.read(move |q| {
            Ok((
                crate::agent_profiles::store::memberships(q, &subject)?.profiles,
                crate::mcp_server::registry::get(q, device, agent)?.is_some(),
            ))
        })
        .await
        .expect("read");
    assert_eq!(member, vec![profile]);
    assert!(still_registered);
}

#[tokio::test]
async fn only_a_blocked_agent_can_be_unblocked_and_it_gets_no_rights() {
    let v = vault();
    let device = v.device;
    v.db.write(move |tx| block(tx, device, "pest", "pest", Transport::Ipc, NOW))
        .await
        .expect("block");
    let blocked =
        v.db.read(move |q| list_with_rights(q, device))
            .await
            .expect("list")[0]
            .0
            .id;
    let registered = registered(&v, NOW).await;

    assert!(!v
        .db
        .write(move |tx| unblock(tx, device, registered))
        .await
        .expect("not blocked")
        .is_some());
    assert!(v
        .db
        .write(move |tx| unblock(tx, device, blocked))
        .await
        .expect("unblock")
        .is_some());
    let left: Vec<Uuid> =
        v.db.read(move |q| list_with_rights(q, device))
            .await
            .expect("list")
            .into_iter()
            .map(|(agent, _, _)| agent.id)
            .collect();
    assert_eq!(left, [registered]);
}

#[tokio::test]
async fn an_unknown_transport_block_stays_visible_and_can_be_removed() {
    let v = vault();
    let device = v.device;
    v.db.write(move |tx| {
        block(tx, device, "future-client", "Future client", Transport::Ipc, NOW)?;
        tx.execute(
            "UPDATE haex_mcp_agents SET transport = 'future-transport' WHERE vault_device_uuid = ?1 AND slug = 'future-client'",
            params![device.to_string()],
        )?;
        Ok(())
    })
    .await
    .expect("future transport row");
    let blocked =
        v.db.read(move |q| {
            Ok(list_with_rights(q, device)?
                .into_iter()
                .find(|(agent, _, _)| agent.slug == "future-client")
                .map(|(agent, _, _)| agent.id))
        })
        .await
        .expect("list")
        .expect("the block remains visible");
    let row =
        v.db.read(move |q| get(q, device, blocked))
            .await
            .expect("read")
            .expect("blocked row");
    assert_eq!(row.transport, Transport::Unknown);
    assert_eq!(row.status, AgentStatus::Blocked);

    let removed =
        v.db.write(move |tx| unblock(tx, device, blocked))
            .await
            .expect("unblock")
            .expect("block existed");
    assert_eq!(removed.0, "future-client");
    assert_eq!(
        removed.1, None,
        "unknown transport has no session decline to clear"
    );
    assert!(v
        .db
        .read(move |q| get(q, device, blocked))
        .await
        .expect("read")
        .is_none());
}

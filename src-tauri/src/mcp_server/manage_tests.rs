//! Tests for what the user changes about an agent (spec 021 US7, T059, T060) against a real
//! temporary vault.

use std::sync::Arc;

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::grants::{default_rights, OperateLevel, Rights};
use super::manage::{
    expire, is_expired, list_with_rights, renew, revoke, set_mode, set_rights, unblock, Renewal,
    EXPIRES_AFTER_MILLIS,
};
use super::registry::{
    block, get, register, AgentKind, AgentMode, AgentStatus, NewAgent, Transport,
};
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
        mode: AgentMode::Manual,
    }
}

async fn registered(v: &Vault, at: i64) -> Uuid {
    let device = v.device;
    v.db.write(move |tx| register(tx, device, &codex(), "hash", &default_rights(), at))
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
        mode: AgentMode::Auto,
    };
    let rights = chosen.clone();
    let renewed =
        v.db.write(move |tx| renew(tx, device, id, &renewal, "new-hash", &rights, NOW + 5))
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
                    mode: AgentMode::Manual,
                },
                "new-hash",
                &default_rights(),
                NOW + 1,
            )
        })
        .await
        .expect("revoked registrations do not renew as a storage error");
    assert_eq!(renewed, None);
}

#[tokio::test]
async fn revoking_forgets_the_agent_its_rights_and_its_file_permissions() {
    let v = vault();
    let id = registered(&v, NOW).await;
    let other = registered(&v, NOW).await;
    let device = v.device;
    v.db.write(move |tx| {
        for agent in [id, other] {
            tx.execute(
                "INSERT INTO agent_file_permissions (id, agent_id, kind, target, status, \
                 updated_at) VALUES (?1, ?2, 'device', 'all', 'granted', ?3)",
                params![Uuid::new_v4().to_string(), agent.to_string(), NOW],
            )?;
        }
        Ok(())
    })
    .await
    .expect("file permissions");

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
                    "SELECT COUNT(*) FROM haex_mcp_agent_grants WHERE agent_id = ?1",
                    id,
                )?,
                count(
                    q,
                    "SELECT COUNT(*) FROM agent_file_permissions WHERE agent_id = ?1",
                    id,
                )?,
                count(
                    q,
                    "SELECT COUNT(*) FROM agent_file_permissions WHERE agent_id = ?1",
                    other,
                )?,
            ])
        })
        .await
        .expect("counts");
    assert_eq!(counts, [0, 0, 0, 1], "another agent keeps its own");
    assert!(!v
        .db
        .write(move |tx| revoke(tx, device, id))
        .await
        .expect("again"));
}

#[tokio::test]
async fn only_a_blocked_agent_can_be_unblocked_and_it_gets_no_rights_or_mode() {
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
        .write(move |tx| set_mode(tx, device, blocked, AgentMode::Auto))
        .await
        .expect("mode"));
    assert!(!v
        .db
        .write(move |tx| set_rights(tx, device, blocked, &default_rights()))
        .await
        .expect("rights"));
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
            .map(|(agent, _)| agent.id)
            .collect();
    assert_eq!(left, [registered]);
}

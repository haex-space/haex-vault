use std::collections::BTreeSet;

use haex_crdt::rusqlite::params;
use tokio::io::{duplex, split};

use super::*;
use crate::storage::query::{self, Query};
use crate::sync::content_keys;
use crate::sync::handshake::local_schema;
use crate::sync::link::join::{self, Join};
use crate::sync::test_support::{Device, Member};

/// A fresh installation: an empty vault of its own and device keys, no
/// identity and no data (what the join flow creates).
pub(super) struct Newcomer {
    pub(super) device: Device,
    pub(super) keys: DeviceKeys,
}

pub(super) fn newcomer() -> Newcomer {
    let device = Device::new();
    let keys = device
        .db()
        .write(|tx| keys::ensure_device_keys(tx, uuid::Uuid::new_v4(), 1))
        .expect("device keys");
    Newcomer { device, keys }
}

pub(super) fn write_thread(device: &Device, id: &str) {
    device
        .db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO chat_threads (id, title, created_at, updated_at) VALUES (?1, ?1, 1, 1)",
                params![id],
            )?;
            Ok(())
        })
        .expect("write thread");
}

pub(super) fn threads(device: &Device) -> Vec<String> {
    query::read(device.db(), |r| {
        r.query_map("SELECT id FROM chat_threads ORDER BY id", &[], |row| {
            row.get(0)
        })
    })
    .expect("read threads")
}

pub(super) fn effective_devices(device: &Device, vault: &[u8; 32]) -> BTreeSet<([u8; 32], bool)> {
    query::read(device.db(), |r| {
        let valid = device_list::valid_lists(&device_list::load_all(r)?, vault);
        Ok(device_list::effective(&valid)
            .map(|s| {
                s.list
                    .devices
                    .iter()
                    .map(|d| (d.device_pubkey, d.role == ListRole::Main))
                    .collect()
            })
            .unwrap_or_default())
    })
    .expect("read list")
}

pub(super) fn pending_of(device: &Device) -> Vec<Pending> {
    query::read(device.db(), |r| pending::load_all(r)).expect("read pending")
}

pub(super) fn vault_secret(device: &Device) -> Option<[u8; 32]> {
    query::read(device.db(), |r| keys::vault_secret(r))
        .expect("read")
        .map(|s| *s)
}

pub(super) struct Run {
    pub(super) host: Result<Outcome, LinkError>,
    pub(super) join: Result<join::Joined, LinkError>,
    pub(super) shown: Option<String>,
}

/// Runs both sides against each other with the given code and decision.
pub(super) async fn link(
    main: &Member,
    joiner: &Newcomer,
    host_code: &LinkCode,
    join_code: &LinkCode,
    decision: Decision,
    joiner_schema: SchemaVersion,
) -> Run {
    let (host_side, join_side) = duplex(1 << 22);
    let (host_recv, host_send) = split(host_side);
    let (mut join_recv, mut join_send) = split(join_side);
    let shown = std::sync::Mutex::new(None);
    let host = Host {
        replica: &main.device.replica,
        keys: &main.keys,
        vault: main.vault,
        code: host_code,
        schema: local_schema(),
        now_ms: 1_000,
    };
    let joining = Join {
        replica: &joiner.device.replica,
        keys: &joiner.keys,
        vault_device_uuid: joiner.device.db().device_id(),
        code: join_code,
        name: "new laptop",
        schema: joiner_schema,
        now_ms: 1_000,
    };
    // As on a real connection, the host's end of the stream closes when
    // its side is over, so a joiner waiting on it learns that.
    let host_side = async {
        let (mut send, mut recv) = (host_send, host_recv);
        run(
            &mut send,
            &mut recv,
            joiner.keys.endpoint_id,
            &host,
            |new| *shown.lock().expect("lock") = Some(new.name),
            async move { decision },
        )
        .await
    };
    let (host_result, join_result) = tokio::join!(
        host_side,
        join::run(
            &mut join_send,
            &mut join_recv,
            main.keys.endpoint_id,
            &joining,
            |_| {},
        ),
    );
    Run {
        host: host_result,
        join: join_result,
        shown: shown.into_inner().expect("lock"),
    }
}

#[tokio::test]
async fn a_new_device_links_as_a_linked_device_without_the_vault_secret() {
    let main = Member::genesis();
    write_thread(&main.device, "t1");
    let joiner = newcomer();
    let code = LinkCode::generate();

    let outcome = link(
        &main,
        &joiner,
        &code,
        &code,
        Decision::Accept { as_main: false },
        local_schema(),
    )
    .await;

    assert_eq!(
        outcome.host.expect("linked"),
        Outcome::Linked {
            device: joiner.keys.device_pubkey,
            name: "new laptop".into()
        }
    );
    assert!(!outcome.join.expect("joined").as_main);
    assert_eq!(outcome.shown.as_deref(), Some("new laptop"));

    assert_eq!(threads(&joiner.device), ["t1"], "the data arrived");
    assert_eq!(
        vault_secret(&joiner.device),
        None,
        "SC-009: no vault secret without the role"
    );
    let expected: BTreeSet<_> = [
        (main.keys.device_pubkey, true),
        (joiner.keys.device_pubkey, false),
    ]
    .into();
    assert_eq!(effective_devices(&main.device, &main.vault), expected);
    assert_eq!(effective_devices(&joiner.device, &main.vault), expected);
    assert!(
        pending_of(&main.device).is_empty(),
        "the host's record is gone"
    );
    let waiting = pending_of(&joiner.device);
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].state, State::AwaitingPublication);
    let key = query::read(joiner.device.db(), |r| content_keys::current_key(r, &[])).expect("read");
    assert!(
        key.is_some(),
        "the content key was wrapped for the new device"
    );
}

#[tokio::test]
async fn a_new_device_can_become_a_main_device_and_then_holds_the_vault_secret() {
    let main = Member::genesis();
    let joiner = newcomer();
    let code = LinkCode::generate();

    let outcome = link(
        &main,
        &joiner,
        &code,
        &code,
        Decision::Accept { as_main: true },
        local_schema(),
    )
    .await;

    assert!(outcome.join.expect("joined").as_main);
    outcome.host.expect("linked");
    assert_eq!(
        vault_secret(&joiner.device),
        vault_secret(&main.device),
        "SC-012: the vault secret arrived over the link"
    );
    assert!(
        effective_devices(&main.device, &main.vault).contains(&(joiner.keys.device_pubkey, true))
    );
}

#[tokio::test]
async fn a_wrong_code_transfers_nothing() {
    let main = Member::genesis();
    write_thread(&main.device, "t1");
    let joiner = newcomer();
    let before = effective_devices(&main.device, &main.vault);

    let outcome = link(
        &main,
        &joiner,
        &LinkCode::generate(),
        &LinkCode::generate(),
        Decision::Accept { as_main: false },
        local_schema(),
    )
    .await;

    assert!(matches!(outcome.host, Err(LinkError::BadProof)));
    assert!(outcome.join.is_err());
    assert_eq!(outcome.shown, None, "the host never showed the name");
    assert!(threads(&joiner.device).is_empty());
    assert!(pending_of(&main.device).is_empty());
    assert_eq!(effective_devices(&main.device, &main.vault), before);
}

#[tokio::test]
async fn a_declined_link_gives_the_new_device_nothing() {
    let main = Member::genesis();
    write_thread(&main.device, "t1");
    let joiner = newcomer();
    let code = LinkCode::generate();
    let before = effective_devices(&main.device, &main.vault);

    let outcome = link(
        &main,
        &joiner,
        &code,
        &code,
        Decision::Reject,
        local_schema(),
    )
    .await;

    assert_eq!(outcome.host.expect("declined"), Outcome::Rejected);
    assert!(matches!(outcome.join, Err(LinkError::Declined)));
    assert!(threads(&joiner.device).is_empty());
    assert!(pending_of(&main.device).is_empty());
    assert!(pending_of(&joiner.device).is_empty());
    assert_eq!(effective_devices(&main.device, &main.vault), before);
}

#[tokio::test]
async fn another_schema_version_ends_the_link_before_the_user_is_asked() {
    let main = Member::genesis();
    let joiner = newcomer();
    let code = LinkCode::generate();
    let mut newer = local_schema();
    newer.haex_migration += 1;

    let outcome = link(
        &main,
        &joiner,
        &code,
        &code,
        Decision::Accept { as_main: false },
        newer,
    )
    .await;

    assert!(matches!(outcome.host, Err(LinkError::Incompatible)));
    assert!(matches!(
        outcome.join,
        Err(LinkError::Aborted(AbortReason::Incompatible))
    ));
    assert_eq!(outcome.shown, None);
    assert!(threads(&joiner.device).is_empty());
}

#[tokio::test]
async fn a_linked_device_cannot_link_another() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    linked.device.pull_from(&main.device);
    let joiner = newcomer();
    let code = LinkCode::generate();

    let outcome = link(
        &linked,
        &joiner,
        &code,
        &code,
        Decision::Accept { as_main: false },
        local_schema(),
    )
    .await;

    assert!(
        matches!(outcome.host, Err(LinkError::NotMainDevice)),
        "{:?}",
        outcome.host
    );
    assert!(outcome.join.is_err());
    assert!(pending_of(&linked.device).is_empty());
    assert!(threads(&joiner.device).is_empty());
}

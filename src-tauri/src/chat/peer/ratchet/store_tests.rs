//! The ratchet rows of one installation (spec 095 T007): every function stays within its
//! installation, so the rows of another installation in a copied vault file are byte-identical
//! after it ran (SC-007), and no row type prints a pickle.

use haex_crdt::rusqlite::types::Value;
use uuid::Uuid;

use super::*;
use crate::chat::peer::test_support::ChatDevice;
use crate::vault_gate::VaultDb;

const PEER: &str = "bb";
const TABLES: [&str; 3] = [
    "chat_ratchet_account_no_sync",
    "chat_ratchet_sessions_no_sync",
    "chat_ratchet_peers_no_sync",
];

fn session(id: &str, secret: &str) -> SessionRow {
    SessionRow {
        peer_device: PEER.to_owned(),
        session_id: id.to_owned(),
        session: Zeroizing::new(secret.to_owned()),
        peer_identity_key: "identity".into(),
        created_at: 1,
        last_received_at: None,
        last_sent_at: Some(2),
        own_ratchet_key: Some("key".into()),
        own_key_since: Some(2),
        renewed_at: None,
    }
}

/// Every row of `installation` in the ratchet tables, column by column.
fn rows_of(db: &VaultDb, installation: Uuid) -> Vec<Vec<String>> {
    db.read_blocking(move |r| {
        let mut all = Vec::new();
        for table in TABLES {
            all.extend(r.query_map(
                &format!("SELECT * FROM {table} WHERE installation_uuid = ?1 ORDER BY 2, 3"),
                params![installation.to_string()],
                |row| {
                    (0..row.as_ref().column_count())
                        .map(|index| row.get::<_, Value>(index).map(|value| format!("{value:?}")))
                        .collect::<haex_crdt::rusqlite::Result<Vec<_>>>()
                },
            )?);
        }
        Ok(all)
    })
    .expect("rows")
}

/// Fills every table for `installation`.
fn seed(db: &VaultDb, installation: Uuid, secret: &'static str) {
    db.write_blocking(move |tx| {
        put_account(
            tx,
            installation,
            &AccountRow {
                account: Zeroizing::new(secret.to_owned()),
                start_key_made_at: 1,
                record_published_at: Some(1),
            },
        )?;
        put_session(tx, installation, &session("s1", secret))?;
        put_session(tx, installation, &session("s2", secret))?;
        put_peer(
            tx,
            installation,
            PEER,
            &PeerRow {
                reset_sent_at: Some(1),
                refetch_record: true,
                ..PeerRow::default()
            },
        )?;
        Ok(())
    })
    .expect("seed");
}

#[test]
fn every_function_stays_within_its_installation() {
    let device = ChatDevice::genesis(&[]);
    let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
    seed(&device.db, other, "the other installation's secret");
    let before = rows_of(&device.db, other);
    assert_eq!(before.len(), 4, "account, two sessions, peer");

    seed(&device.db, mine, "my secret");
    let read = device
        .db
        .write_blocking(move |tx| {
            let mut changed = session("s1", "my advanced secret");
            changed.renewed_at = Some(9);
            put_session(tx, mine, &changed)?;
            put_peer(tx, mine, PEER, &PeerRow::default())?;
            let found = (
                account(tx, mine)?.expect("my account"),
                sessions(tx, mine, PEER)?,
                peer(tx, mine, PEER)?,
            );
            delete_session(tx, mine, PEER, "s2")?;
            let after_one = sessions(tx, mine, PEER)?.len();
            delete_sessions(tx, mine, PEER)?;
            Ok((found, after_one))
        })
        .expect("my rows");
    let ((account_row, my_sessions, my_peer), after_one) = read;
    assert_eq!(account_row.account.as_str(), "my secret");
    assert_eq!(
        my_sessions
            .iter()
            .map(|row| (
                row.session_id.as_str(),
                row.session.as_str(),
                row.renewed_at
            ))
            .collect::<Vec<_>>(),
        [
            ("s1", "my advanced secret", Some(9)),
            ("s2", "my secret", None)
        ],
        "by session id, the advanced state over the old one"
    );
    assert_eq!(my_peer, PeerRow::default());
    assert_eq!(after_one, 1);

    assert_eq!(rows_of(&device.db, other), before, "SC-007");
    let none_left = device
        .db
        .read_blocking(move |r| Ok(sessions(r, mine, PEER)?))
        .expect("read");
    assert!(none_left.is_empty());
}

#[test]
fn an_installation_without_rows_has_no_account_and_a_default_peer() {
    let device = ChatDevice::genesis(&[]);
    seed(&device.db, Uuid::new_v4(), "someone else's");
    let fresh = Uuid::new_v4();
    let (found_account, found_sessions, found_peer) = device
        .db
        .read_blocking(move |r| {
            Ok((
                account(r, fresh)?.is_some(),
                sessions(r, fresh, PEER)?.len(),
                peer(r, fresh, PEER)?,
            ))
        })
        .expect("read");
    assert!(!found_account);
    assert_eq!(found_sessions, 0);
    assert_eq!(found_peer, PeerRow::default());
}

#[test]
fn no_row_type_prints_its_pickle() {
    let row = session("s1", "geheimer-zustand");
    assert!(!format!("{row:?}").contains("geheimer-zustand"));
    let account_row = AccountRow {
        account: Zeroizing::new("geheimes-konto".into()),
        start_key_made_at: 1,
        record_published_at: None,
    };
    assert!(!format!("{account_row:?}").contains("geheimes-konto"));
}

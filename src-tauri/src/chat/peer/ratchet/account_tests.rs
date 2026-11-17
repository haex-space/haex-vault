//! The ratchet account of an installation (spec 095 T008): made once, with one start key, renewed
//! so that an opening for the previous start key still opens a session and one for the start key
//! before it no longer does; another installation gets its own account (scenario 5).

use uuid::Uuid;
use vodozemac::olm::{OlmMessage, SessionConfig};

use super::*;
use crate::chat::peer::test_support::ChatDevice;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn keys(device: &ChatDevice, installation: Uuid, now: i64) -> OwnKeys {
    device
        .db
        .write_blocking(move |tx| Ok(own_keys(&load_or_create(tx, installation, now)?)?))
        .expect("account")
}

fn renew(device: &ChatDevice, installation: Uuid, now: i64) {
    device
        .db
        .write_blocking(move |tx| Ok(renew_start_key(tx, installation, now)?))
        .expect("renew");
}

/// Whether the stored account of `installation` opens a session from `opening`; nothing is saved.
fn opens(device: &ChatDevice, installation: Uuid, opening: &OlmMessage) -> bool {
    let OlmMessage::PreKey(opening) = opening.clone() else {
        panic!("an opening message")
    };
    device
        .db
        .read_blocking(move |r| {
            let (mut account, _) = load(r, installation)?.expect("account");
            Ok(account
                .create_inbound_session(
                    SessionConfig::version_1(),
                    opening.identity_key(),
                    &opening,
                )
                .is_ok())
        })
        .expect("read")
}

#[test]
fn an_account_is_made_once_with_one_start_key() {
    let device = ChatDevice::genesis(&[]);
    let installation = Uuid::new_v4();
    let first = keys(&device, installation, 1_000);
    let again = keys(&device, installation, 2_000);
    assert_eq!(first, again, "the second call loads the account");
    let made_at = device
        .db
        .read_blocking(move |r| Ok(load(r, installation)?.expect("account").1))
        .expect("read");
    assert_eq!(made_at, 1_000);
}

#[test]
fn an_opening_for_the_previous_start_key_still_opens_and_one_two_keys_back_does_not() {
    let device = ChatDevice::genesis(&[]);
    let installation = Uuid::new_v4();
    let offered = keys(&device, installation, 0);
    let sender = vodozemac::olm::Account::new();
    let mut session = sender
        .create_outbound_session(SessionConfig::version_1(), offered.identity, offered.start)
        .expect("session");
    let opening = session.encrypt("hallo").expect("encrypt");
    assert!(opens(&device, installation, &opening));

    renew(&device, installation, 30 * DAY_MS);
    let renewed = keys(&device, installation, 30 * DAY_MS);
    assert_ne!(renewed.start, offered.start, "a new start key");
    assert_eq!(renewed.identity, offered.identity, "the same identity");
    assert!(
        opens(&device, installation, &opening),
        "the previous start key is kept for one more period"
    );
    let (made_at, published) = device
        .db
        .read_blocking(move |r| {
            let row = store::account(r, installation)?.expect("row");
            Ok((row.start_key_made_at, row.record_published_at))
        })
        .expect("read");
    assert_eq!(made_at, 30 * DAY_MS);
    assert_eq!(
        published, None,
        "the new start key still has to be published"
    );

    renew(&device, installation, 60 * DAY_MS);
    assert!(
        !opens(&device, installation, &opening),
        "two renewals later the start key is gone"
    );
}

/// Scenario 5: a vault file copied to another installation makes its own account there and leaves
/// the first installation's row as it is.
#[test]
fn another_installation_gets_its_own_account_and_leaves_the_first_one_alone() {
    let device = ChatDevice::genesis(&[]);
    let (first, copy) = (Uuid::new_v4(), Uuid::new_v4());
    let first_keys = keys(&device, first, 1);
    let read_first = |device: &ChatDevice| {
        device
            .db
            .read_blocking(move |r| {
                let row = store::account(r, first)?.expect("row");
                Ok((row.account.to_string(), row.start_key_made_at))
            })
            .expect("read")
    };
    let before = read_first(&device);

    let copy_keys = keys(&device, copy, 2);
    assert_ne!(copy_keys.identity, first_keys.identity);
    assert_eq!(read_first(&device), before);
}

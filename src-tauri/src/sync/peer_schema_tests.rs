use super::*;
use crate::sync::problems::{self, Problem};
use crate::sync::test_support::Device;

fn device() -> [u8; 32] {
    [7; 32]
}

fn state(migration: u32) -> SchemaVersion {
    SchemaVersion {
        protocol: 2,
        holzi_migration: migration,
        crdt_trigger: 27,
    }
}

fn read(d: &Device) -> Option<SchemaVersion> {
    crate::storage::query::read(d.db(), |r| of(r, &device())).expect("read")
}

#[test]
fn a_state_is_unknown_until_a_handshake_recorded_it() {
    let d = Device::new();
    assert_eq!(read(&d), None);

    assert!(record(&d.replica, &device(), state(40)).expect("record"));

    assert_eq!(read(&d), Some(state(40)));
}

#[test]
fn recording_the_same_state_again_changes_nothing() {
    let d = Device::new();
    record(&d.replica, &device(), state(40)).expect("record");

    assert!(!record(&d.replica, &device(), state(40)).expect("again"));
}

#[test]
fn a_new_state_replaces_all_three_values() {
    let d = Device::new();
    record(&d.replica, &device(), state(39)).expect("record");
    let newer = SchemaVersion {
        protocol: 3,
        holzi_migration: 41,
        crdt_trigger: 28,
    };

    assert!(record(&d.replica, &device(), newer).expect("replace"));

    assert_eq!(read(&d), Some(newer));
}

#[test]
fn a_row_with_a_missing_value_reads_as_unknown() {
    let d = Device::new();
    record(&d.replica, &device(), state(40)).expect("record");
    d.replica
        .db()
        .write(|tx| {
            tx.execute(
                "UPDATE device_presence_no_sync SET peer_trigger = NULL WHERE device_pubkey = ?1",
                haex_crdt::rusqlite::params![device().as_slice()],
            )?;
            Ok(())
        })
        .expect("clear one");

    assert_eq!(read(&d), None);
}

#[test]
fn recording_leaves_the_time_and_the_problem_and_they_leave_it() {
    let d = Device::new();
    crate::sync::seen::touch(&d.replica, &device(), 1_000).expect("touch");
    problems::set(&d.replica, &device(), Problem::IncompatibleVersion).expect("problem");

    record(&d.replica, &device(), state(40)).expect("record");

    let (seen, problem) = crate::storage::query::read(d.db(), |r| {
        let seen: Option<i64> = r.query_row(
            "SELECT last_seen FROM device_presence_no_sync WHERE device_pubkey = ?1",
            haex_crdt::rusqlite::params![device().as_slice()],
            |row| row.get(0),
        )?;
        Ok((seen, problems::of(r, &device())?))
    })
    .expect("read");
    assert_eq!(seen, Some(1_000));
    assert_eq!(problem, Some(Problem::IncompatibleVersion));

    crate::sync::seen::touch(&d.replica, &device(), 2_000).expect("touch later");
    problems::clear(&d.replica, &device()).expect("clear problem");
    assert_eq!(read(&d), Some(state(40)), "time and problem keep the state");
}

#[test]
fn removing_a_device_drops_its_state_with_the_row() {
    let d = Device::new();
    record(&d.replica, &device(), state(40)).expect("record");

    d.replica
        .db()
        .write(|tx| {
            tx.execute(
                "DELETE FROM device_presence_no_sync WHERE device_pubkey = ?1",
                haex_crdt::rusqlite::params![device().as_slice()],
            )?;
            Ok(())
        })
        .expect("delete");

    assert_eq!(read(&d), None);
}

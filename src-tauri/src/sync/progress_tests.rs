use haex_crdt::rusqlite::params;

use super::*;
use crate::storage::query;
use crate::sync::replica::synced_tables;
use crate::sync::test_support::Device;

fn write_thread(device: &Device, id: &str, title: &str) {
    device
        .db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO chat_threads (id, title, created_at, updated_at) VALUES (?1, ?2, 1, 1) \
                 ON CONFLICT(id) DO UPDATE SET title = excluded.title",
                params![id, title],
            )?;
            Ok(())
        })
        .expect("write thread");
}

fn thread_hlc(device: &Device, id: &str) -> String {
    query::read(device.db(), |r| {
        r.query_row(
            "SELECT haex_hlc_no_sync FROM chat_threads WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map(|hlc| hlc.expect("row"))
    })
    .expect("read hlc")
}

#[test]
fn the_origin_of_an_own_write_is_the_vault_device_uuid() {
    let device = Device::new();
    write_thread(&device, "t1", "one");
    assert_eq!(
        origin_of(&thread_hlc(&device, "t1")),
        Some(device.db().device_id())
    );
    assert_eq!(origin_of("not an hlc"), None);
}

#[test]
fn own_progress_is_the_newest_own_synced_cell() {
    let device = Device::new();
    let own = device.db().device_id();
    write_thread(&device, "t1", "one");
    let first = thread_hlc(&device, "t1");
    assert_eq!(
        device.replica.progress().expect("progress").get(&own),
        Some(&first)
    );

    write_thread(&device, "t2", "two");
    let second = thread_hlc(&device, "t2");
    assert_eq!(
        device.replica.progress().expect("progress").get(&own),
        Some(&second),
        "a later call finds cells written since the first"
    );
}

#[test]
fn a_write_to_a_device_local_table_does_not_move_own_progress() {
    let device = Device::new();
    let own = device.db().device_id();
    write_thread(&device, "t1", "one");
    let before = device.replica.progress().expect("progress");

    device
        .db()
        .write(|tx| {
            tx.execute(
                "INSERT INTO haex_maintenance_no_sync (task) VALUES ('test-task')",
                &[],
            )?;
            Ok(())
        })
        .expect("device-local write");

    assert_eq!(
        device.replica.progress().expect("progress").get(&own),
        before.get(&own)
    );
}

#[test]
fn stored_progress_only_rises() {
    let device = Device::new();
    let origin = Uuid::new_v4();
    let low = "7000000000000000000/1".to_string();
    let high = "7000000000000000009/1".to_string();
    let advance_to = |hlc: &str| {
        device
            .db()
            .write(|tx| advance(tx, &Vector::from([(origin, hlc.to_string())])))
            .expect("advance");
    };

    advance_to(&high);
    advance_to(&low);

    let stored = query::read(device.db(), |r| stored(r)).expect("stored");
    assert_eq!(stored.get(&origin), Some(&high));
}

#[test]
fn more_means_beyond_ours_for_some_origin() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let ours = Vector::from([(a, "5/1".to_string())]);
    assert!(!has_more(&Vector::from([(a, "5/1".to_string())]), &ours));
    assert!(!has_more(&Vector::from([(a, "4/1".to_string())]), &ours));
    assert!(has_more(&Vector::from([(a, "6/1".to_string())]), &ours));
    assert!(has_more(&Vector::from([(b, "1/2".to_string())]), &ours));
    assert!(!has_more(&Vector::new(), &ours));
}

#[test]
fn hlcs_compare_by_time_not_by_text() {
    assert!(is_beyond("100/1", Some(&"99/1".to_string())));
    assert!(!is_beyond("99/1", Some(&"100/1".to_string())));
    assert!(is_beyond("1/1", None));
}

#[test]
fn synced_tables_leave_out_device_local_tables() {
    let device = Device::new();
    let tables = query::read(device.db(), |r| synced_tables(r)).expect("tables");
    for synced in [
        "chat_threads",
        "providers",
        "haex_deleted_rows",
        "device_lists",
    ] {
        assert!(tables.iter().any(|t| t == synced), "{synced} travels");
    }
    assert!(tables.iter().all(|t| !t.ends_with("_no_sync")));
}

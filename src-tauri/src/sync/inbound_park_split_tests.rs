//! Rows whose cells came split across groups, one of them parked (spec 017, research R10): the
//! row waits whole and is written whole.

use super::super::*;
use super::{count, ddl, parked, threads, write, write_thread};
use crate::sync::test_support::Device;

const NOTES: &str = "CREATE TABLE t:notes (id TEXT PRIMARY KEY, title TEXT NOT NULL, body TEXT)";

#[test]
fn a_row_completed_by_a_parked_group_waits_whole_while_core_data_arrives() {
    let (a, b) = (Device::new(), Device::new());
    ddl(&a, NOTES);
    ddl(&b, NOTES);
    write(
        &a,
        "INSERT INTO t:notes (id, title, body) VALUES ('n1', 'first', 'text')",
    );
    // B does not have the new column yet: the group that sets title with it parks, and the
    // first group then carries only the body.
    ddl(&a, "ALTER TABLE t:notes ADD COLUMN tag TEXT");
    write(
        &a,
        "UPDATE t:notes SET title = 'second', tag = 'x' WHERE id = 'n1'",
    );
    write_thread(&a, "core");

    b.pull_from(&a);
    assert_eq!(threads(&b), 1, "core data in the same pull is applied");
    assert_eq!(count(&b, "SELECT COUNT(*) FROM t:notes"), 0);
    assert_eq!(parked(&b).len(), 2, "both groups of the row wait");

    b.pull_from(&a);
    ddl(&b, "ALTER TABLE t:notes ADD COLUMN tag TEXT");
    replay_ready(b.db(), &|| false).expect("replay");
    assert_eq!(
        count(
            &b,
            "SELECT COUNT(*) FROM t:notes \
             WHERE title = 'second' AND tag = 'x' AND body = 'text'"
        ),
        1
    );
    assert!(parked(&b).is_empty());
}

#[test]
fn a_row_held_on_an_earlier_page_parks_with_the_group_that_completes_it() {
    let (a, b) = (Device::new(), Device::new());
    ddl(&a, NOTES);
    ddl(&b, NOTES);
    write(
        &a,
        "INSERT INTO t:notes (id, title, body) VALUES ('n1', 'first', 'text')",
    );
    ddl(&a, "ALTER TABLE t:notes ADD COLUMN tag TEXT");
    write(
        &a,
        "UPDATE t:notes SET title = 'second', tag = 'x' WHERE id = 'n1'",
    );
    write_thread(&a, "core");

    // One change per page: the first group is held on its own page before the second parks.
    let received = b.try_pull_from(&a, 1).expect("pull");
    assert!(received.len() > 2, "the groups came on separate pages");
    assert_eq!(threads(&b), 1);
    assert_eq!(count(&b, "SELECT COUNT(*) FROM t:notes"), 0);
    assert_eq!(parked(&b).len(), 2, "the held group parked with the row");

    ddl(&b, "ALTER TABLE t:notes ADD COLUMN tag TEXT");
    replay_ready(b.db(), &|| false).expect("replay");
    assert_eq!(
        count(
            &b,
            "SELECT COUNT(*) FROM t:notes \
             WHERE title = 'second' AND tag = 'x' AND body = 'text'"
        ),
        1
    );
}

#[test]
fn parked_groups_that_only_together_make_a_row_replay_together() {
    let (a, b) = (Device::new(), Device::new());
    ddl(&a, NOTES);
    write(
        &a,
        "INSERT INTO t:notes (id, title, body) VALUES ('n1', 'first', 'text')",
    );
    write(&a, "UPDATE t:notes SET title = 'second' WHERE id = 'n1'");

    b.pull_from(&a);
    assert_eq!(parked(&b).len(), 2);

    ddl(&b, NOTES);
    let replayed = replay_ready(b.db(), &|| false).expect("replay");
    assert_eq!(replayed.groups, 2);
    assert_eq!(
        count(
            &b,
            "SELECT COUNT(*) FROM t:notes WHERE title = 'second' AND body = 'text'"
        ),
        1
    );
    assert!(parked(&b).is_empty());
}

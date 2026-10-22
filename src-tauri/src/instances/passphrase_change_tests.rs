//! Tests for checking the current vault passphrase against the file (spec 042, spec 049 T063): the
//! External Bridge asks the same check for a passkey that requires user verification.

use haex_crdt::rusqlite::Connection;

use super::passphrase::Passphrase;
use super::verify_current;
use crate::error::HolziError;

#[test]
fn the_right_passphrase_opens_the_file_and_a_wrong_one_is_named() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    let conn = Connection::open(&path).expect("open");
    conn.pragma_update(None, "key", "richtig-lang-genug")
        .expect("key");
    conn.execute("CREATE TABLE t (x INTEGER)", [])
        .expect("write");
    drop(conn);

    assert!(verify_current(&path, &Passphrase::from("richtig-lang-genug")).is_ok());
    let wrong = verify_current(&path, &Passphrase::from("falsch")).map_err(HolziError::from);
    assert!(
        matches!(wrong, Err(HolziError::WrongPassphrase)),
        "{wrong:?}"
    );
}

//! The switch "Geräte-Dateizugriff" (FR-014): on by default, off persists, and it is no vault data
//! another device could change.

use super::{enabled, set_enabled};
use crate::remote_storage::test_support::vault;

#[test]
fn the_switch_is_on_until_turned_off_here() {
    let (_dir, db) = vault();
    assert!(db.read_blocking(|q| Ok(enabled(q)?)).expect("read"));
    db.write_blocking(|tx| Ok(set_enabled(tx, false, "2026-10-23T10:00:00.000Z")?))
        .expect("off");
    assert!(!db.read_blocking(|q| Ok(enabled(q)?)).expect("read"));
    db.write_blocking(|tx| Ok(set_enabled(tx, true, "2026-10-23T10:01:00.000Z")?))
        .expect("on");
    assert!(db.read_blocking(|q| Ok(enabled(q)?)).expect("read"));
}

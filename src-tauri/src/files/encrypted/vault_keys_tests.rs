use super::*;
use crate::files::encrypted::test_support::give_key;
use crate::remote_storage::test_support::vault;

#[test]
fn a_device_without_a_content_key_has_none() {
    let (_dir, db) = vault();
    assert!(db.read_blocking(|q| current(q)).expect("read").is_none());
    assert!(db
        .read_blocking(|q| HeldKeys::load(q))
        .expect("read")
        .is_empty());
    assert_eq!(no_key().code, crate::files::FilesErrorCode::NoKey);
}

#[test]
fn a_new_folder_takes_the_newest_generation_and_old_ones_still_open() {
    let (_dir, db) = vault();
    let first = give_key(&db, 1);
    let second = give_key(&db, 2);
    let newest = db
        .read_blocking(|q| current(q))
        .expect("read")
        .expect("a key");
    assert_eq!(newest.key_id, second.key_id);
    let held = db.read_blocking(|q| HeldKeys::load(q)).expect("read");
    assert_eq!(held.by_key_id(&first.key_id).as_deref(), Some(&*first.key));
    assert_eq!(
        held.by_key_id(&second.key_id).as_deref(),
        Some(&*second.key)
    );
    assert!(held.by_key_id(&[0; 16]).is_none());
}

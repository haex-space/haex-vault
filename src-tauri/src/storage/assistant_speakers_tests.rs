//! Spec 066 FR-016a, T174 (storage part): enrolled people keep name, print and model; renaming and
//! deleting work by id.

use uuid::Uuid;

use super::assistant_speakers::{delete, insert, list, rename};
use super::query;
use crate::passwords::test_support::open_test_vault;

#[test]
fn a_person_keeps_their_voice_print() {
    let (_dir, db) = open_test_vault();
    let print = [0.25f32, -0.5, 1.0, f32::MIN_POSITIVE];
    let anna = db
        .write(|tx| insert(tx, "Anna", "campplus-en-voxceleb", &print, 4, 7))
        .expect("insert");
    let ben = db
        .write(|tx| insert(tx, "ben", "campplus-en-voxceleb", &[0.1], 3, 8))
        .expect("insert");
    let listed = query::read(&db, |r| list(r)).expect("list");
    assert_eq!(listed, vec![anna.clone(), ben.clone()], "by name, any case");
    assert_eq!(
        listed[0].embedding, print,
        "the print comes back bit for bit"
    );
    assert_eq!(listed[0].utterances, 4);
}

#[test]
fn a_person_is_renamed_and_deleted_by_id() {
    let (_dir, db) = open_test_vault();
    let anna = db
        .write(|tx| insert(tx, "Anna", "m", &[1.0], 3, 7))
        .expect("insert");
    assert!(db
        .write(|tx| rename(tx, anna.id, "Anna B."))
        .expect("rename"));
    assert_eq!(
        query::read(&db, |r| list(r)).expect("list")[0].name,
        "Anna B."
    );
    assert!(!db
        .write(|tx| rename(tx, Uuid::new_v4(), "X"))
        .expect("rename unknown"));
    assert!(db.write(|tx| delete(tx, anna.id)).expect("delete"));
    assert!(!db.write(|tx| delete(tx, anna.id)).expect("delete again"));
    assert!(query::read(&db, |r| list(r)).expect("list").is_empty());
}

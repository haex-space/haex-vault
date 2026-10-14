use super::*;
use crate::contacts::phone::store::Base;

fn link(raw: i64, version: i64) -> Link {
    Link {
        contact_id: format!("c{raw}"),
        raw_contact_id: raw,
        lookup_key: String::new(),
        account_type: String::new(),
        account_name: String::new(),
        source_id: None,
        version,
        read_only: false,
        base: Base::default(),
        photo_base: None,
    }
}

fn raw(id: i64, version: i64, deleted: bool) -> RawIndex {
    RawIndex {
        id,
        version,
        deleted,
        account_type: String::new(),
        account_name: String::new(),
        source_id: None,
        contact_id: id,
        lookup_key: format!("l{id}"),
        last_updated: None,
        read_only: false,
    }
}

#[test]
fn sorts_the_index_against_the_links() {
    let links = vec![link(1, 3), link(2, 5), link(3, 1), link(4, 2)];
    let index = vec![
        raw(1, 3, false),
        raw(2, 6, false),
        raw(4, 3, true),
        raw(5, 1, false),
    ];
    let out = classify(&links, &index);
    assert_eq!(
        out.new.iter().map(|raw| raw.id).collect::<Vec<_>>(),
        vec![5],
        "a new id"
    );
    assert_eq!(
        out.changed.iter().map(|raw| raw.id).collect::<Vec<_>>(),
        vec![2],
        "a higher version"
    );
    assert_eq!(
        out.unchanged.iter().map(|raw| raw.id).collect::<Vec<_>>(),
        vec![1]
    );
    let mut gone: Vec<i64> = out.gone.iter().map(|link| link.raw_contact_id).collect();
    gone.sort_unstable();
    assert_eq!(gone, vec![3, 4], "a missing id and DELETED=1");
}

#[test]
fn an_own_write_with_its_stored_version_is_no_change() {
    // holzi wrote raw 7 and stored the version the phone answered.
    let out = classify(&[link(7, 12)], &[raw(7, 12, false)]);
    assert!(out.changed.is_empty());
    assert_eq!(out.unchanged.len(), 1);
}

#[test]
fn a_deleted_raw_contact_holzi_never_linked_is_ignored() {
    let out = classify(&[], &[raw(9, 2, true)]);
    assert_eq!(out, Classified::default());
}

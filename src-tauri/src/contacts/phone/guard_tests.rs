use super::*;
use crate::contacts::phone::store::Base;

fn link(raw: i64, account: (&str, &str)) -> Link {
    Link {
        contact_id: format!("c{raw}"),
        raw_contact_id: raw,
        lookup_key: String::new(),
        account_type: account.0.to_owned(),
        account_name: account.1.to_owned(),
        source_id: None,
        version: 1,
        read_only: false,
        base: Base::default(),
        photo_base: None,
    }
}

fn raw(id: i64, account: (&str, &str)) -> RawIndex {
    RawIndex {
        id,
        version: 1,
        deleted: false,
        account_type: account.0.to_owned(),
        account_name: account.1.to_owned(),
        source_id: None,
        contact_id: id,
        lookup_key: format!("l{id}"),
        last_updated: None,
        read_only: false,
    }
}

fn listed(account: (&str, &str)) -> PhoneAccount {
    PhoneAccount {
        account_type: account.0.to_owned(),
        name: account.1.to_owned(),
        writable: true,
        is_default: false,
    }
}

#[test]
fn more_than_ten_and_more_than_a_tenth_needs_a_decision() {
    assert!(is_mass_delete(11, 100));
    assert!(!is_mass_delete(10, 50), "ten are never a mass deletion");
    assert!(!is_mass_delete(11, 200), "eleven of 200 are not a tenth");
    assert!(is_mass_delete(11, 0));
}

#[test]
fn an_account_that_left_the_phone_is_no_deletion() {
    let work = ("com.work", "mara@work.example");
    let gone = vec![link(1, work), link(2, work), link(3, ("", ""))];
    let index = vec![raw(4, ("", ""))];
    let vanished = vanished_accounts(&gone, &index, &[listed(("", ""))]);
    assert_eq!(
        vanished,
        BTreeSet::from([(work.0.to_owned(), work.1.to_owned())])
    );
}

#[test]
fn an_account_the_phone_still_lists_did_not_vanish() {
    let work = ("com.work", "mara@work.example");
    let vanished = vanished_accounts(&[link(1, work)], &[], &[listed(work)]);
    assert!(
        vanished.is_empty(),
        "its contacts were deleted, the account is still there"
    );
}

#[test]
fn an_account_with_raw_contacts_left_did_not_vanish() {
    let work = ("com.work", "mara@work.example");
    let vanished = vanished_accounts(&[link(1, work)], &[raw(2, work)], &[]);
    assert!(vanished.is_empty());
}

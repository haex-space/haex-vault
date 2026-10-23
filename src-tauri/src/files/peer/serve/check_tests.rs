//! The decision per request (FR-004, FR-005, FR-007).

use super::{allowed, hides_own, own_data};
use crate::files::peer::wire::{Caller, FailCode, Mode, Op};
use crate::files::share::store::FileShareRow;
use crate::files::share::{FileShareItemKind, FileShareLevel, FileShareSourceState};
use crate::files::SourceRef;

fn share() -> FileShareRow {
    FileShareRow {
        id: "share-1".to_owned(),
        vault_device_uuid: "device".to_owned(),
        source: SourceRef::Device,
        path: "/home/a/Fotos".to_owned(),
        item_kind: FileShareItemKind::Folder,
        name: "Fotos".to_owned(),
        created_at: "t".to_owned(),
        changed_at: "t".to_owned(),
        served_at: None,
        source_state: FileShareSourceState::Ok,
    }
}

fn read(share: Option<&str>) -> Op {
    Op::ListDir {
        share: share.map(str::to_owned),
        path: "/".to_owned(),
        max: 10,
    }
}

#[test]
fn a_device_with_the_switch_off_gets_nothing() {
    assert_eq!(
        allowed(Mode::AccessOff, &read(None), None),
        Err(FailCode::Refused)
    );
    assert_eq!(
        allowed(Mode::AccessOff, &Op::Shares, None),
        Err(FailCode::Refused)
    );
}

#[test]
fn an_own_device_reaches_the_device_but_names_no_share() {
    assert_eq!(allowed(Mode::OwnDevice, &read(None), None), Ok(()));
    assert_eq!(
        allowed(Mode::OwnDevice, &read(Some("share-1")), None),
        Err(FailCode::Refused)
    );
}

#[test]
fn a_contact_needs_the_grant_read_for_this_request() {
    let row = share();
    assert_eq!(allowed(Mode::Contact, &Op::Shares, None), Ok(()));
    assert_eq!(
        allowed(Mode::Contact, &read(Some("share-1")), None),
        Err(FailCode::Refused),
        "a revoked grant refuses the next request (FR-024)"
    );
    assert_eq!(
        allowed(
            Mode::Contact,
            &read(Some("share-1")),
            Some((&row, FileShareLevel::Read))
        ),
        Ok(())
    );
}

#[test]
fn holzis_own_data_is_read_only_for_the_user_and_closed_to_everyone_else() {
    assert_eq!(own_data(Mode::OwnDevice, Caller::User, false, true), Ok(()));
    assert_eq!(
        own_data(Mode::OwnDevice, Caller::User, true, true),
        Err(FailCode::Refused)
    );
    assert_eq!(
        own_data(Mode::OwnDevice, Caller::Agent, false, true),
        Err(FailCode::Refused)
    );
    assert_eq!(
        own_data(Mode::Contact, Caller::User, false, true),
        Err(FailCode::Refused)
    );
    assert_eq!(own_data(Mode::Contact, Caller::User, true, false), Ok(()));
    assert!(!hides_own(Mode::OwnDevice, Caller::User));
    assert!(hides_own(Mode::OwnDevice, Caller::Agent));
    assert!(hides_own(Mode::Contact, Caller::User));
}

//! The decision per request (FR-004, FR-005, FR-007).

use super::{allowed, at_root, hides_own, own_data};
use crate::files::peer::wire::{Caller, Expected, FailCode, Mode, Op};
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

fn upload(expected: Expected) -> Op {
    Op::BeginUpload {
        share: Some("share-1".to_owned()),
        path: "/Brief.pdf".to_owned(),
        expected,
    }
}

#[test]
fn a_shared_file_is_read_and_written_over_but_never_moved_or_deleted() {
    let mut row = share();
    row.item_kind = FileShareItemKind::File;
    let grant = Some((&row, FileShareLevel::ReadWrite));
    let version = Expected::Version {
        size: 4,
        etag: "e".to_owned(),
    };
    assert_eq!(allowed(Mode::Contact, &upload(version), grant), Ok(()));
    assert_eq!(
        allowed(Mode::Contact, &upload(Expected::Absent), grant),
        Err(FailCode::Refused),
        "nothing new beside the shared file"
    );
    let changes = [
        Op::CreateFolder {
            share: Some("share-1".to_owned()),
            path: "/neu".to_owned(),
        },
        Op::Move {
            share: Some("share-1".to_owned()),
            from: "/Brief.pdf".to_owned(),
            to: "/Anders.pdf".to_owned(),
        },
        Op::Delete {
            share: Some("share-1".to_owned()),
            path: "/Brief.pdf".to_owned(),
        },
    ];
    for op in changes {
        assert_eq!(
            allowed(Mode::Contact, &op, grant),
            Err(FailCode::Refused),
            "{op:?}"
        );
    }
}

#[test]
fn reading_a_share_never_writes() {
    let row = share();
    let delete = Op::Delete {
        share: Some("share-1".to_owned()),
        path: "/a.jpg".to_owned(),
    };
    assert_eq!(
        allowed(Mode::Contact, &delete, Some((&row, FileShareLevel::Read))),
        Err(FailCode::Refused)
    );
    assert_eq!(
        allowed(
            Mode::Contact,
            &delete,
            Some((&row, FileShareLevel::ReadWrite))
        ),
        Ok(())
    );
    let part = Op::UploadPart {
        upload_id: "u".to_owned(),
        number: 1,
        len: 1,
    };
    assert_eq!(
        allowed(Mode::Contact, &part, Some((&row, FileShareLevel::Read))),
        Err(FailCode::Refused),
        "a part is checked against the upload's share too"
    );
}

#[test]
fn the_root_itself_is_never_moved_deleted_or_written() {
    let at_the_root = [
        Op::Delete {
            share: None,
            path: "/".to_owned(),
        },
        Op::Move {
            share: None,
            from: "/".to_owned(),
            to: "/x".to_owned(),
        },
        Op::CreateFolder {
            share: None,
            path: "/".to_owned(),
        },
        upload(Expected::Absent),
    ];
    for op in &at_the_root {
        assert_eq!(at_root(op, ""), Err(FailCode::Refused), "{op:?}");
        assert_eq!(at_root(op, "Fotos"), Ok(()), "{op:?}");
    }
    let list = read(None);
    assert_eq!(at_root(&list, ""), Ok(()), "the root is listed");
}

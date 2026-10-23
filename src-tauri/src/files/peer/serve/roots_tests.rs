//! Where window paths lead (research R3) and the lexical half of the containment catalog (SC-005);
//! links out of a root are refused by `LocalStore` and tested with the round trip in
//! `files/peer/serve_tests.rs`.

use super::{in_share, on_device, parts};
use crate::files::peer::wire::FailCode;
use crate::files::share::store::FileShareRow;
use crate::files::share::{FileShareItemKind, FileShareSourceState};
use crate::files::SourceRef;

fn share(path: &str, item_kind: FileShareItemKind, source: SourceRef) -> FileShareRow {
    FileShareRow {
        id: "share-1".to_owned(),
        vault_device_uuid: "device".to_owned(),
        source,
        path: path.to_owned(),
        item_kind,
        name: "x".to_owned(),
        created_at: "t".to_owned(),
        changed_at: "t".to_owned(),
        served_at: None,
        source_state: FileShareSourceState::Ok,
    }
}

#[test]
fn dot_dot_dot_backslash_nul_and_relative_paths_are_refused() {
    for path in [
        "a/b",
        "",
        "/..",
        "/a/../b",
        "/a/..",
        "/./a",
        "/a/.",
        "/a\\..\\b",
        "/a\0b",
    ] {
        assert_eq!(parts(path), Err(FailCode::Refused), "{path:?}");
    }
    assert_eq!(
        parts("//a///b/"),
        Ok(vec!["a", "b"]),
        "empty parts are skipped"
    );
    assert_eq!(parts("/"), Ok(vec![]));
}

#[cfg(not(windows))]
#[test]
fn an_own_device_path_starts_at_the_file_system_root() {
    let located = on_device("/home/a/Dokumente").expect("located");
    assert_eq!(located.root, std::path::PathBuf::from("/"));
    assert_eq!(located.key, "home/a/Dokumente");
    assert_eq!(located.folder_prefix(), "home/a/Dokumente/");
    let root = on_device("/").expect("root");
    assert_eq!(root.key, "");
    assert_eq!(root.folder_prefix(), "");
}

#[cfg(not(windows))]
#[test]
fn a_contact_starts_at_the_shared_folder() {
    let row = share(
        "/home/a/Fotos",
        FileShareItemKind::Folder,
        SourceRef::Device,
    );
    let located = in_share(&row, "/Urlaub/1.jpg").expect("located");
    assert_eq!(located.root, std::path::PathBuf::from("/home/a/Fotos"));
    assert_eq!(located.key, "Urlaub/1.jpg");
    assert_eq!(
        located.path(),
        std::path::PathBuf::from("/home/a/Fotos/Urlaub/1.jpg")
    );
    assert_eq!(in_share(&row, "/../b"), Err(FailCode::Refused));
}

#[cfg(not(windows))]
#[test]
fn a_shared_file_is_the_only_thing_its_contact_sees() {
    let row = share(
        "/home/a/Brief.pdf",
        FileShareItemKind::File,
        SourceRef::Device,
    );
    let root = in_share(&row, "/").expect("root");
    assert_eq!(root.root, std::path::PathBuf::from("/home/a"));
    assert_eq!(root.key, "");
    assert_eq!(root.only.as_deref(), Some("Brief.pdf"));
    assert_eq!(in_share(&row, "/Brief.pdf").expect("file").key, "Brief.pdf");
    assert_eq!(
        in_share(&row, "/Other.pdf"),
        Err(FailCode::NotFound),
        "a neighbour of the shared file does not exist for the contact"
    );
    assert_eq!(in_share(&row, "/Brief.pdf/x"), Err(FailCode::NotFound));
}

#[test]
fn a_share_of_a_storage_comes_later() {
    let row = share(
        "/Fotos",
        FileShareItemKind::Folder,
        SourceRef::Storage {
            storage_id: "s3".to_owned(),
        },
    );
    assert_eq!(in_share(&row, "/"), Err(FailCode::Unsupported));
}

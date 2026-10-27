//! Only folders of this device are watched.

use super::*;

#[test]
fn a_storage_place_is_not_watched() {
    let place = SyncRulePlace {
        source: SourceRef::Storage {
            storage_id: "s3".to_owned(),
        },
        path: "/x".to_owned(),
    };
    assert!(watch_place(&place, Arc::new(|| {})).is_none());
}

#[test]
fn a_device_folder_is_watched() {
    let dir = tempfile::tempdir().unwrap();
    let place = SyncRulePlace {
        source: SourceRef::Device,
        path: dir.path().to_string_lossy().into_owned(),
    };
    assert!(watch_place(&place, Arc::new(|| {})).is_some());
}

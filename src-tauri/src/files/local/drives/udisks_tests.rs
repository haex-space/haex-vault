use std::collections::HashMap;

use zbus::names::OwnedInterfaceName;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use super::*;

#[test]
fn udisks_errors_map_to_what_the_window_shows() {
    let e = |name: &str| map_error(name, "text of the system");
    assert_eq!(
        e("org.freedesktop.UDisks2.Error.NotAuthorizedDismissed"),
        DriveError::Cancelled
    );
    assert_eq!(
        e("org.freedesktop.UDisks2.Error.Cancelled"),
        DriveError::Cancelled
    );
    assert_eq!(
        e("org.freedesktop.UDisks2.Error.DeviceBusy"),
        DriveError::Busy
    );
    assert_eq!(
        e("org.freedesktop.DBus.Error.UnknownObject"),
        DriveError::Gone
    );
    assert_eq!(
        e("org.freedesktop.UDisks2.Error.Failed"),
        DriveError::Failed("text of the system".to_owned())
    );
    assert_eq!(
        map_error("org.freedesktop.UDisks2.Error.NotAuthorized", ""),
        DriveError::Failed("org.freedesktop.UDisks2.Error.NotAuthorized".to_owned())
    );
}

fn owned(value: Value<'_>) -> OwnedValue {
    value.try_to_owned().unwrap()
}

fn interface(
    name: &str,
    props: Vec<(&str, OwnedValue)>,
) -> (OwnedInterfaceName, HashMap<String, OwnedValue>) {
    (
        OwnedInterfaceName::try_from(name).unwrap(),
        props.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
    )
}

fn path(text: &str) -> OwnedValue {
    owned(Value::from(ObjectPath::try_from(text).unwrap()))
}

#[test]
fn managed_objects_become_udisks_objects() {
    let mut managed: ManagedObjects = HashMap::new();
    managed.insert(
        OwnedObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sdb1").unwrap(),
        [
            interface(
                BLOCK,
                vec![
                    ("IdLabel", owned(Value::from("FOTOS"))),
                    ("IdUUID", owned(Value::from("C0DE-F00D"))),
                    ("HintIgnore", owned(Value::from(false))),
                    ("HintSystem", owned(Value::from(false))),
                    ("CryptoBackingDevice", path("/")),
                    ("Size", owned(Value::from(32_000_000u64))),
                    ("Drive", path("/org/freedesktop/UDisks2/drives/stick")),
                ],
            ),
            interface(
                FILESYSTEM,
                vec![(
                    "MountPoints",
                    owned(Value::from(vec![b"/run/media/anna/FOTOS\0".to_vec()])),
                )],
            ),
        ]
        .into_iter()
        .collect(),
    );
    managed.insert(
        OwnedObjectPath::try_from("/org/freedesktop/UDisks2/drives/stick").unwrap(),
        [interface(
            DRIVE,
            vec![
                ("Removable", owned(Value::from(true))),
                ("CanPowerOff", owned(Value::from(true))),
                ("Ejectable", owned(Value::from(false))),
            ],
        )]
        .into_iter()
        .collect(),
    );
    let objects = to_objects(managed);
    let block = objects.iter().find(|o| o.path.ends_with("/sdb1")).unwrap();
    let props = block.block.as_ref().unwrap();
    assert_eq!(props.label, "FOTOS");
    assert_eq!(props.uuid, "C0DE-F00D");
    assert!(!props.hint_system);
    assert_eq!(props.crypto_backing, None, "`/` means none");
    assert_eq!(props.size, 32_000_000);
    assert_eq!(
        props.drive.as_deref(),
        Some("/org/freedesktop/UDisks2/drives/stick")
    );
    assert_eq!(
        block.filesystem.as_ref().unwrap().mount_points,
        ["/run/media/anna/FOTOS"]
    );
    let drive = objects.iter().find(|o| o.path.ends_with("/stick")).unwrap();
    let props = drive.drive.as_ref().unwrap();
    assert!(props.removable && props.can_power_off && !props.ejectable);
}

#[test]
fn an_unmounted_file_system_has_no_mount_points() {
    let mut managed: ManagedObjects = HashMap::new();
    let empty: Vec<Vec<u8>> = Vec::new();
    managed.insert(
        OwnedObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sdc1").unwrap(),
        [interface(
            FILESYSTEM,
            vec![("MountPoints", owned(Value::from(empty)))],
        )]
        .into_iter()
        .collect(),
    );
    let objects = to_objects(managed);
    assert!(objects[0]
        .filesystem
        .as_ref()
        .unwrap()
        .mount_points
        .is_empty());
    assert!(objects[0].block.is_none());
}

#[test]
fn a_wrong_password_is_an_unlock_failure_with_the_system_s_text() {
    assert_eq!(
        unlock_error(
            "org.freedesktop.UDisks2.Error.Failed",
            "Error unlocking /dev/loop0: Failed to activate device: Operation not permitted",
        ),
        DriveError::UnlockFailed(
            "Error unlocking /dev/loop0: Failed to activate device: Operation not permitted"
                .to_owned()
        )
    );
    assert_eq!(
        unlock_error("org.freedesktop.UDisks2.Error.NotAuthorizedDismissed", ""),
        DriveError::Cancelled
    );
    assert_eq!(
        unlock_error("org.freedesktop.DBus.Error.UnknownObject", ""),
        DriveError::Gone
    );
}

#[test]
fn unlocked_content_without_a_file_system_is_not_opened() {
    let content = |filesystem: Option<FsProps>| UdisksObject {
        path: "/org/freedesktop/UDisks2/block_devices/dm_2d0".to_owned(),
        block: Some(BlockProps::default()),
        filesystem,
        ..UdisksObject::default()
    };
    let path = "/org/freedesktop/UDisks2/block_devices/dm_2d0";
    assert!(has_filesystem(&[content(Some(FsProps::default()))], path));
    // LVM or another volume inside (FR-020).
    assert!(!has_filesystem(&[content(None)], path));
    assert!(!has_filesystem(&[], path));
}

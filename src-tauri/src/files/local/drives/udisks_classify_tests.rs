use super::*;
use crate::files::local::drives::{invariants_hold, sort_drives};

const BLOCKS: &str = "/org/freedesktop/UDisks2/block_devices";
const DRIVES: &str = "/org/freedesktop/UDisks2/drives";

fn block(name: &str, props: BlockProps) -> UdisksObject {
    UdisksObject {
        path: format!("{BLOCKS}/{name}"),
        block: Some(props),
        ..UdisksObject::default()
    }
}

fn with_fs(mut object: UdisksObject, mount: Option<&str>) -> UdisksObject {
    object.filesystem = Some(FsProps {
        mount_points: mount.map(str::to_owned).into_iter().collect(),
    });
    object
}

fn locked(mut object: UdisksObject) -> UdisksObject {
    object.encrypted = Some(EncProps { cleartext: None });
    object
}

fn unlocked_as(mut object: UdisksObject, cleartext: &str) -> UdisksObject {
    object.encrypted = Some(EncProps {
        cleartext: Some(format!("{BLOCKS}/{cleartext}")),
    });
    object
}

fn drive(name: &str, removable: bool) -> UdisksObject {
    UdisksObject {
        path: format!("{DRIVES}/{name}"),
        drive: Some(DriveProps {
            removable,
            can_power_off: removable,
            ejectable: false,
        }),
        ..UdisksObject::default()
    }
}

fn on(name: &str, props: BlockProps) -> BlockProps {
    BlockProps {
        drive: Some(format!("{DRIVES}/{name}")),
        ..props
    }
}

fn props(label: &str, uuid: &str) -> BlockProps {
    BlockProps {
        label: label.to_owned(),
        uuid: uuid.to_owned(),
        hint_system: true,
        size: 1_000,
        ..BlockProps::default()
    }
}

fn removable(label: &str, uuid: &str) -> BlockProps {
    BlockProps {
        hint_system: false,
        ..props(label, uuid)
    }
}

/// This machine on 2026-10-27 (`lsblk`): NVMe with ESP, recovery, LUKS with LVM and root, plain
/// dm-crypt swap; a two-partition USB stick; plus the cases the spec names.
fn machine() -> Vec<UdisksObject> {
    vec![
        drive("nvme", false),
        drive("usb_mint", true),
        drive("usb_fotos", true),
        drive("usb_backup", true),
        drive("sata", false),
        // NVMe: ESP and recovery are mounted at system places.
        with_fs(
            block("nvme0n1p1", on("nvme", props("", "AAAA-BBBB"))),
            Some("/boot/efi"),
        ),
        with_fs(
            block("nvme0n1p2", on("nvme", props("", "1111"))),
            Some("/recovery"),
        ),
        // LUKS with LVM inside: the content block holds no file system.
        unlocked_as(
            block("nvme0n1p3", on("nvme", props("", "LUKS-ROOT"))),
            "dm_2d0",
        ),
        block(
            "dm_2d0",
            BlockProps {
                crypto_backing: Some(format!("{BLOCKS}/nvme0n1p3")),
                ..props("", "")
            },
        ),
        // The root logical volume: no crypto backing of its own.
        with_fs(block("dm_2d1", props("", "ROOT-FS")), Some("/")),
        // Plain dm-crypt swap: neither encrypted interface nor file system.
        block("nvme0n1p4", on("nvme", props("", ""))),
        block("dm_2d2", props("", "")),
        // The Mint stick: iso9660 and a writable partition, not mounted.
        with_fs(
            block(
                "sdc1",
                on(
                    "usb_mint",
                    removable("Linux Mint 22.3 Cinnamon 64-bit", "2026-01-01"),
                ),
            ),
            None,
        ),
        with_fs(
            block("sdc3", on("usb_mint", removable("writable", "W-UUID"))),
            None,
        ),
        // An unmounted vfat stick.
        with_fs(
            block("sdd1", on("usb_fotos", removable("FOTOS", "C0DE-F00D"))),
            None,
        ),
        // A locked LUKS stick.
        locked(block(
            "sde1",
            on("usb_backup", removable("", "LUKS-BACKUP")),
        )),
        // An internal NTFS partition, not mounted, HintSystem as on every internal disk.
        with_fs(
            block("sda2", on("sata", props("Windows", "NTFS-UUID"))),
            None,
        ),
        // Ignored by a udev rule.
        with_fs(
            block(
                "sda1",
                on(
                    "sata",
                    BlockProps {
                        hint_ignore: true,
                        ..props("SYSTEM", "IGN")
                    },
                ),
            ),
            None,
        ),
    ]
}

fn listed(objects: &[UdisksObject]) -> Vec<Drive> {
    let mut drives = classify(objects);
    sort_drives(&mut drives);
    drives
}

fn by_label<'a>(drives: &'a [Drive], label: &str) -> &'a Drive {
    drives
        .iter()
        .find(|d| d.label.as_deref() == Some(label))
        .unwrap_or_else(|| panic!("{label} missing in {drives:#?}"))
}

#[test]
fn exactly_the_drives_a_user_looks_into_appear() {
    let drives = listed(&machine());
    let ids: Vec<&str> = drives
        .iter()
        .map(|d| d.id.trim_start_matches(&format!("{BLOCKS}/")))
        .collect();
    // No ESP, recovery, LVM content, swap or ignored block; HintSystem hides nothing.
    assert_eq!(ids, ["dm_2d1", "sda2", "sdd1", "sdc1", "sdc3", "sde1"]);
    for drive in &drives {
        assert!(invariants_hold(drive), "{drive:?}");
    }
}

#[test]
fn the_root_is_the_system_drive_without_a_button() {
    let drives = listed(&machine());
    let root = &drives[0];
    assert_eq!(root.kind, DriveKind::System);
    assert_eq!(root.state, DriveState::Mounted);
    assert_eq!(root.path.as_deref(), Some("/"));
    assert_eq!(root.release, DriveRelease::None);
}

#[test]
fn an_internal_partition_shows_unmounted_and_internal() {
    let drives = listed(&machine());
    let windows = by_label(&drives, "Windows");
    assert_eq!(windows.kind, DriveKind::Internal);
    assert_eq!(windows.state, DriveState::Unmounted);
    assert_eq!(windows.key.as_deref(), Some("ntfs-uuid"));
}

#[test]
fn each_partition_of_a_stick_is_its_own_removable_drive() {
    let drives = listed(&machine());
    for label in ["Linux Mint 22.3 Cinnamon 64-bit", "writable", "FOTOS"] {
        let drive = by_label(&drives, label);
        assert_eq!(drive.kind, DriveKind::Removable, "{label}");
        assert_eq!(drive.state, DriveState::Unmounted, "{label}");
        assert_eq!(drive.release, DriveRelease::None, "{label}");
    }
}

#[test]
fn a_locked_stick_is_locked_and_keyed_by_its_container() {
    let drives = listed(&machine());
    let locked = drives.iter().find(|d| d.id.ends_with("/sde1")).unwrap();
    assert_eq!(locked.state, DriveState::Locked);
    assert!(locked.encrypted);
    assert_eq!(locked.key.as_deref(), Some("luks-backup"));
    assert_eq!(locked.label, None);
    assert_eq!(locked.kind, DriveKind::Removable);
}

#[test]
fn an_unlocked_stick_keeps_the_container_key_and_shows_its_label() {
    let mut objects = machine();
    let at = objects
        .iter()
        .position(|o| o.path.ends_with("/sde1"))
        .unwrap();
    objects[at] = unlocked_as(
        block("sde1", on("usb_backup", removable("", "LUKS-BACKUP"))),
        "dm_2d5",
    );
    objects.push(with_fs(
        block(
            "dm_2d5",
            BlockProps {
                crypto_backing: Some(format!("{BLOCKS}/sde1")),
                ..props("BACKUP", "EXT4-INNER")
            },
        ),
        Some("/run/media/anna/BACKUP"),
    ));
    let drives = listed(&objects);
    let backup = by_label(&drives, "BACKUP");
    assert_eq!(backup.key.as_deref(), Some("luks-backup"));
    assert!(backup.encrypted);
    assert_eq!(backup.state, DriveState::Mounted);
    assert_eq!(backup.kind, DriveKind::Removable);
    assert_eq!(backup.release, DriveRelease::Eject);
    assert!(!drives.iter().any(|d| d.id.ends_with("/sde1")));
}

#[test]
fn a_mounted_internal_partition_can_be_unmounted() {
    let objects = vec![
        drive("sata", false),
        with_fs(
            block("sdb1", on("sata", props("data", "D1"))),
            Some("/mnt/data"),
        ),
    ];
    let drives = listed(&objects);
    assert_eq!(drives[0].release, DriveRelease::Unmount);
    assert_eq!(drives[0].kind, DriveKind::Internal);
}

#[test]
fn a_loop_image_without_a_drive_is_removable() {
    let objects = vec![with_fs(
        block("loop0", removable("E2EFOTOS", "ABCD-0001")),
        Some("/media/x"),
    )];
    let drives = listed(&objects);
    assert_eq!(drives[0].kind, DriveKind::Removable);
    assert_eq!(drives[0].release, DriveRelease::Eject);
}

#[test]
fn home_itself_is_hidden() {
    let objects = vec![
        drive("nvme", false),
        with_fs(block("home", on("nvme", props("", "H"))), Some("/home")),
    ];
    assert!(listed(&objects).is_empty());
}

/// `machine()` with the file system of the block `name` mounted at `point`.
fn mount(objects: &mut [UdisksObject], name: &str, point: &str) {
    let object = objects
        .iter_mut()
        .find(|object| object.path == format!("{BLOCKS}/{name}"))
        .unwrap();
    object.filesystem = Some(FsProps {
        mount_points: vec![point.to_owned()],
    });
}

fn steps_of(objects: &[UdisksObject], name: &str) -> ReleaseSteps {
    let drive = classify(objects)
        .into_iter()
        .find(|drive| drive.id == format!("{BLOCKS}/{name}"))
        .unwrap();
    release_steps(objects, &drive).unwrap()
}

#[test]
fn a_mounted_stick_is_unmounted_and_switched_off() {
    let mut objects = machine();
    mount(&mut objects, "sdd1", "/media/u/FOTOS");
    assert_eq!(
        steps_of(&objects, "sdd1"),
        ReleaseSteps {
            unmount: format!("{BLOCKS}/sdd1"),
            lock: None,
            power: Some(Power::Off(format!("{DRIVES}/usb_fotos"))),
        }
    );
}

#[test]
fn a_stick_with_another_mounted_partition_stays_on() {
    let mut objects = machine();
    mount(&mut objects, "sdc1", "/media/u/Mint");
    mount(&mut objects, "sdc3", "/media/u/writable");
    assert_eq!(steps_of(&objects, "sdc3").power, None);

    let mut objects = machine();
    mount(&mut objects, "sdc3", "/media/u/writable");
    assert_eq!(
        steps_of(&objects, "sdc3").power,
        Some(Power::Off(format!("{DRIVES}/usb_mint")))
    );
}

#[test]
fn an_unlocked_stick_is_unmounted_locked_and_switched_off() {
    let mut objects = machine();
    let at = objects
        .iter()
        .position(|object| object.path.ends_with("/sde1"))
        .unwrap();
    objects[at] = unlocked_as(objects[at].clone(), "dm_2d3");
    objects.push(with_fs(
        block(
            "dm_2d3",
            BlockProps {
                crypto_backing: Some(format!("{BLOCKS}/sde1")),
                ..removable("BACKUP", "EXT4-UUID")
            },
        ),
        Some("/media/u/BACKUP"),
    ));
    assert_eq!(
        steps_of(&objects, "dm_2d3"),
        ReleaseSteps {
            unmount: format!("{BLOCKS}/dm_2d3"),
            lock: Some(format!("{BLOCKS}/sde1")),
            power: Some(Power::Off(format!("{DRIVES}/usb_backup"))),
        }
    );
}

#[test]
fn an_internal_partition_and_a_loop_image_are_only_unmounted() {
    let mut objects = machine();
    mount(&mut objects, "sda2", "/mnt/windows");
    objects.push(with_fs(
        block("loop0", removable("E2EFOTOS", "1234-5678")),
        Some("/media/u/E2EFOTOS"),
    ));
    assert_eq!(steps_of(&objects, "sda2").power, None);
    assert_eq!(steps_of(&objects, "loop0").power, None);
}

#[test]
fn a_drive_that_cannot_power_off_is_ejected() {
    let mut objects = machine();
    mount(&mut objects, "sdd1", "/media/u/FOTOS");
    for object in &mut objects {
        if let Some(drive) = object
            .drive
            .as_mut()
            .filter(|_| object.path.ends_with("usb_fotos"))
        {
            drive.can_power_off = false;
            drive.ejectable = true;
        }
    }
    assert_eq!(
        steps_of(&objects, "sdd1").power,
        Some(Power::Eject(format!("{DRIVES}/usb_fotos")))
    );
}

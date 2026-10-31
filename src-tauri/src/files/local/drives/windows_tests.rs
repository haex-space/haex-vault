use super::*;
use crate::files::local::drives::invariants_hold;

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn the_path_names_split_at_nul_and_drop_the_empty_ends() {
    // As recorded on Windows Server 2025 (spike T070): `C:\` and the two NUL that end the list.
    let mut buffer = utf16("C:\\");
    buffer.extend([0, 0]);
    assert_eq!(path_names(&buffer), ["C:\\"]);

    let mut both = utf16("E:\\");
    both.push(0);
    both.extend(utf16("C:\\mnt\\stick\\"));
    both.extend([0, 0]);
    assert_eq!(path_names(&both), ["E:\\", "C:\\mnt\\stick\\"]);

    // A volume without a letter (the EFI partition) has none.
    assert!(path_names(&[0]).is_empty());
    assert!(path_names(&[]).is_empty());
}

#[test]
fn only_a_drive_letter_root_is_a_drive() {
    assert!(is_drive_root("E:\\"));
    assert!(is_drive_root("c:\\"));
    assert!(!is_drive_root("E:"));
    assert!(!is_drive_root("C:\\mnt\\stick\\"));
    assert!(!is_drive_root("\\\\?\\Volume{x}\\"));
}

#[test]
fn the_bus_decides_removable_and_the_windows_directory_system() {
    // USB, SD, MMC are removable; SAS (10), NVMe (17) and a VHD (15) as on the runner are not.
    for bus in [7, 12, 13] {
        assert_eq!(
            kind_of(Some(bus), false, false),
            DriveKind::Removable,
            "{bus}"
        );
    }
    for bus in [10, 15, 17, 11] {
        assert_eq!(
            kind_of(Some(bus), false, false),
            DriveKind::Internal,
            "{bus}"
        );
    }
    assert_eq!(kind_of(Some(17), true, false), DriveKind::Removable);
    assert_eq!(kind_of(None, false, false), DriveKind::Internal);
    assert_eq!(kind_of(Some(7), true, true), DriveKind::System);
}

#[test]
fn a_usb_stick_is_mounted_at_its_letter_and_ejects() {
    let drive = drive_of(VolumeFacts {
        root: "E:\\".into(),
        label: Some("E2ESTICK".into()),
        serial: Some(0x5059_12f3),
        bus: Some(7),
        space: Some((64 << 20, 60 << 20)),
        ..Default::default()
    });
    assert!(invariants_hold(&drive), "{drive:?}");
    assert_eq!(drive.id, "E:\\");
    assert_eq!(drive.path.as_deref(), Some("E:\\"));
    assert_eq!(drive.key.as_deref(), Some("505912f3"));
    assert_eq!(drive.state, DriveState::Mounted);
    assert_eq!(drive.kind, DriveKind::Removable);
    assert_eq!(drive.release, DriveRelease::Eject);
    assert_eq!((drive.total, drive.free), (Some(64 << 20), Some(60 << 20)));
}

#[test]
fn the_system_drive_and_an_internal_one_offer_no_button() {
    let system = drive_of(VolumeFacts {
        root: "C:\\".into(),
        label: Some("Windows".into()),
        serial: Some(0x2eaa_d2fb),
        bus: Some(10),
        system: true,
        ..Default::default()
    });
    assert!(invariants_hold(&system));
    assert_eq!(system.kind, DriveKind::System);
    assert_eq!(system.release, DriveRelease::None);
    let data = drive_of(VolumeFacts {
        root: "D:\\".into(),
        label: Some(String::new()),
        bus: Some(17),
        ..Default::default()
    });
    assert_eq!(data.kind, DriveKind::Internal);
    assert_eq!(data.release, DriveRelease::None);
    assert_eq!(data.label, None, "an empty label is none");
}

#[test]
fn a_locked_bitlocker_volume_is_locked_without_a_path() {
    let drive = drive_of(VolumeFacts {
        root: "B:\\".into(),
        locked: true,
        bus: Some(7),
        ..Default::default()
    });
    assert!(invariants_hold(&drive), "{drive:?}");
    assert_eq!(drive.state, DriveState::Locked);
    assert_eq!(drive.path, None);
    assert!(drive.encrypted);
    assert_eq!(drive.key, None);
    assert_eq!(drive.release, DriveRelease::None);
    assert_eq!(FVE_E_LOCKED_VOLUME, 2_150_694_912, "the code the spike saw");
}

#[test]
fn a_veto_says_why_the_drive_stayed() {
    assert_eq!(veto_error(0, ""), None);
    // An open handle, an application, a service: busy.
    for veto in [2, 3, 4, 5, 6, 7] {
        assert_eq!(veto_error(veto, "x"), Some(DriveError::Busy), "{veto}");
    }
    assert_eq!(veto_error(13, "x"), Some(DriveError::Gone));
    // What a VHD answers (spike T070): not ejectable at all.
    assert!(matches!(
        veto_error(8, "SCSI\\Disk&Ven_Msft&Prod_Virtual_Disk"),
        Some(DriveError::Failed(message)) if message.contains("veto 8")
    ));
}

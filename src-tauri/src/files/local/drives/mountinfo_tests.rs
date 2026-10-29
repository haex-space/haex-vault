use std::path::{Path, PathBuf};

use super::*;

const MOUNTINFO: &str = "\
22 1 259:2 / / rw,relatime shared:1 - ext4 /dev/nvme0n1p2 rw
23 22 0:21 / /proc rw,nosuid shared:12 - proc proc rw
24 22 0:22 / /sys rw,nosuid shared:2 - sysfs sysfs rw
25 22 0:5 / /dev rw,nosuid shared:3 - devtmpfs udev rw
30 22 0:26 / /run rw,nosuid shared:5 - tmpfs tmpfs rw
40 22 259:3 / /home rw,relatime shared:30 - btrfs /dev/nvme0n1p3 rw
51 30 8:17 / /run/media/anna/USB\\040Stick rw,nosuid shared:40 - vfat /dev/sdb1 rw
52 22 8:33 / /mnt/backup rw shared:41 - ext4 /dev/sdc1 rw
53 22 7:1 / /snap/core/1 ro shared:42 - squashfs /dev/loop1 ro
";

#[test]
fn mountinfo_gives_mount_point_and_type() {
    let mounts = parse_mountinfo(MOUNTINFO);
    assert_eq!(mounts.len(), 9);
    assert_eq!(
        mounts[0],
        Mount {
            point: PathBuf::from("/"),
            fs_type: "ext4".to_owned()
        }
    );
}

#[test]
fn octal_escapes_in_mount_points_are_decoded() {
    let mounts = parse_mountinfo(MOUNTINFO);
    assert!(mounts
        .iter()
        .any(|m| m.point == Path::new("/run/media/anna/USB Stick")));
}

#[test]
fn virtual_and_system_mounts_are_no_user_drives() {
    let drives: Vec<PathBuf> = parse_mountinfo(MOUNTINFO)
        .into_iter()
        .filter(is_user_drive)
        .map(|m| m.point)
        .collect();
    assert_eq!(
        drives,
        vec![
            PathBuf::from("/"),
            PathBuf::from("/home"),
            PathBuf::from("/run/media/anna/USB Stick"),
            PathBuf::from("/mnt/backup"),
        ]
    );
}

#[test]
fn a_broken_line_is_skipped() {
    assert!(parse_mountinfo("no separator here\n").is_empty());
}

#[test]
fn the_list_shows_user_drives_without_home_each_once() {
    let twice = format!("{MOUNTINFO}54 22 8:33 / /mnt/backup rw shared:43 - ext4 /dev/sdc1 rw\n");
    assert_eq!(
        user_mount_points(&twice),
        vec![
            PathBuf::from("/"),
            PathBuf::from("/run/media/anna/USB Stick"),
            PathBuf::from("/mnt/backup"),
        ]
    );
}

#[test]
fn home_itself_is_not_shown_but_a_mount_below_it_is() {
    assert!(!is_shown_mount_point(Path::new("/home")));
    assert!(is_shown_mount_point(Path::new("/home/anna/mnt/stick")));
    assert!(!is_shown_mount_point(Path::new("/boot/efi")));
    assert!(!is_shown_mount_point(Path::new("/recovery")));
}

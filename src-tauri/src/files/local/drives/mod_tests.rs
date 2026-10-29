use super::*;

fn drive(id: &str, kind: DriveKind, label: Option<&str>) -> Drive {
    Drive {
        id: id.to_owned(),
        key: None,
        label: label.map(str::to_owned),
        path: None,
        kind,
        state: DriveState::Unmounted,
        release: DriveRelease::None,
        encrypted: false,
        size: None,
        total: None,
        free: None,
    }
}

#[test]
fn drives_are_sorted_by_kind_then_label_then_id() {
    let mut drives = vec![
        drive("e", DriveKind::Removable, None),
        drive("d", DriveKind::Removable, Some("zeta")),
        drive("c", DriveKind::Removable, Some("Alpha")),
        drive("b", DriveKind::Internal, Some("data")),
        drive("a", DriveKind::System, None),
        drive("f", DriveKind::Removable, None),
    ];
    sort_drives(&mut drives);
    let ids: Vec<&str> = drives.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, ["a", "b", "c", "d", "e", "f"]);
}

#[test]
fn the_display_name_is_the_label_else_the_folder_else_the_id() {
    let mut d = drive("/dev/sdb1", DriveKind::Removable, Some("FOTOS"));
    assert_eq!(display_name(&d), "FOTOS");
    d.label = Some(String::new());
    d.path = Some("/run/media/anna/USB Stick".to_owned());
    assert_eq!(display_name(&d), "USB Stick");
    d.path = Some("/".to_owned());
    assert_eq!(display_name(&d), "/");
    d.path = None;
    assert_eq!(display_name(&d), "/dev/sdb1");
}

#[test]
fn only_mounted_drives_reach_agents_and_peers() {
    let mut mounted = drive("m", DriveKind::Removable, None);
    mounted.state = DriveState::Mounted;
    mounted.path = Some("/media/m".to_owned());
    let mut locked = drive("l", DriveKind::Removable, None);
    locked.state = DriveState::Locked;
    locked.encrypted = true;
    let unmounted = drive("u", DriveKind::Internal, None);
    let ids: Vec<String> = mounted_only(vec![locked, mounted, unmounted])
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(ids, ["m"]);
}

#[test]
fn the_invariants_accept_valid_drives() {
    let unmounted = drive("u", DriveKind::Internal, None);
    assert!(invariants_hold(&unmounted));
    let mut locked = drive("l", DriveKind::Removable, None);
    locked.state = DriveState::Locked;
    locked.encrypted = true;
    assert!(invariants_hold(&locked));
    let mut mounted = drive("m", DriveKind::Removable, None);
    mounted.state = DriveState::Mounted;
    mounted.path = Some("/media/m".to_owned());
    mounted.release = DriveRelease::Eject;
    mounted.total = Some(10);
    mounted.free = Some(5);
    assert!(invariants_hold(&mounted));
}

#[test]
fn the_invariants_reject_each_broken_rule() {
    let mut no_path = drive("m", DriveKind::Internal, None);
    no_path.state = DriveState::Mounted;
    assert!(!invariants_hold(&no_path));

    let mut path_but_unmounted = drive("u", DriveKind::Internal, None);
    path_but_unmounted.path = Some("/mnt/u".to_owned());
    assert!(!invariants_hold(&path_but_unmounted));

    let mut ejectable_system = drive("s", DriveKind::System, None);
    ejectable_system.state = DriveState::Mounted;
    ejectable_system.path = Some("/".to_owned());
    ejectable_system.release = DriveRelease::Unmount;
    assert!(!invariants_hold(&ejectable_system));

    let mut button_while_unmounted = drive("u", DriveKind::Removable, None);
    button_while_unmounted.release = DriveRelease::Eject;
    assert!(!invariants_hold(&button_while_unmounted));

    let mut space_while_unmounted = drive("u", DriveKind::Removable, None);
    space_while_unmounted.free = Some(1);
    assert!(!invariants_hold(&space_while_unmounted));

    let mut locked_plain = drive("l", DriveKind::Removable, None);
    locked_plain.state = DriveState::Locked;
    assert!(!invariants_hold(&locked_plain));
}

#[cfg(unix)]
#[test]
fn the_space_of_a_real_folder_is_known() {
    let dir = tempfile::tempdir().unwrap();
    let (total, free) = space(dir.path()).unwrap();
    assert!(total > 0);
    assert!(free <= total);
}

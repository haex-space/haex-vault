use super::*;

const OTHER_KINDS: [PermissionKind; 5] = [
    PermissionKind::Microphone,
    PermissionKind::DisplayCapture,
    PermissionKind::Geolocation,
    PermissionKind::Notifications,
    PermissionKind::Other,
];

#[test]
fn a_closed_gate_denies_the_camera() {
    let gate = CameraGate::default();

    assert_eq!(
        gate.answer(PermissionKind::Camera),
        PermissionResponse::Deny
    );
}

#[test]
fn an_open_gate_allows_the_camera() {
    let gate = CameraGate::default();

    gate.begin();

    assert_eq!(
        gate.answer(PermissionKind::Camera),
        PermissionResponse::Allow
    );
}

#[test]
fn an_open_gate_allows_nothing_else() {
    let gate = CameraGate::default();
    gate.begin();

    for kind in OTHER_KINDS {
        assert_eq!(gate.answer(kind), PermissionResponse::Default, "{kind}");
    }
}

#[test]
fn a_closed_gate_denies_the_camera_and_leaves_other_requests_to_the_system() {
    let gate = CameraGate::default();

    assert_eq!(
        gate.answer(PermissionKind::Camera),
        PermissionResponse::Deny
    );
    for kind in OTHER_KINDS {
        assert_eq!(gate.answer(kind), PermissionResponse::Default, "{kind}");
    }
}

#[test]
fn ending_closes_the_gate_and_ending_twice_is_fine() {
    let gate = CameraGate::default();
    gate.begin();

    gate.end();
    gate.end();

    assert_eq!(
        gate.answer(PermissionKind::Camera),
        PermissionResponse::Deny
    );
}

#[test]
fn beginning_twice_needs_only_one_end() {
    let gate = CameraGate::default();
    gate.begin();
    gate.begin();

    gate.end();

    assert_eq!(
        gate.answer(PermissionKind::Camera),
        PermissionResponse::Deny
    );
}

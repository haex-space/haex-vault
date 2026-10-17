use std::path::PathBuf;

use uuid::Uuid;

use super::*;

fn device() -> Uuid {
    Uuid::from_u128(0x1111)
}

fn other_device() -> Uuid {
    Uuid::from_u128(0x2222)
}

fn permission(
    kind: &str,
    action: &str,
    target: &str,
    status: &str,
    scope_device: Uuid,
) -> Permission {
    Permission::from_row(kind, action, target, status, scope_device)
        .unwrap_or_else(|| panic!("{kind} {action} {target} {status} parses"))
}

fn web_get(url: &str) -> PermissionRequest {
    PermissionRequest {
        kind: PermissionKind::Web,
        action: Action::Method("GET".to_string()),
        target: RequestTarget::Url(WebRequest::parse(url).expect("url")),
    }
}

fn read_path(path: &str) -> PermissionRequest {
    PermissionRequest {
        kind: PermissionKind::Filesystem,
        action: Action::Read,
        target: RequestTarget::Path(PathBuf::from(path)),
    }
}

fn write_path(path: &str) -> PermissionRequest {
    PermissionRequest {
        kind: PermissionKind::Filesystem,
        action: Action::ReadWrite,
        target: RequestTarget::Path(PathBuf::from(path)),
    }
}

#[test]
fn no_candidate_asks() {
    assert_eq!(
        evaluate(&[], &web_get("https://x.test/"), device()),
        Decision::Prompt
    );
}

#[test]
fn a_matching_grant_allows_and_ask_prompts() {
    let granted = [permission("web", "*", "x.test", "granted", VAULT_WIDE)];
    assert_eq!(
        evaluate(&granted, &web_get("https://x.test/"), device()),
        Decision::Allow
    );
    let ask = [permission("web", "*", "x.test", "ask", VAULT_WIDE)];
    assert_eq!(
        evaluate(&ask, &web_get("https://x.test/"), device()),
        Decision::Prompt
    );
}

#[test]
fn denied_beats_granted_and_granted_beats_ask() {
    let candidates = [
        permission("web", "*", "*", "granted", VAULT_WIDE),
        permission("web", "GET", "x.test", "denied", VAULT_WIDE),
        permission("web", "*", "x.test", "ask", VAULT_WIDE),
    ];
    assert_eq!(
        evaluate(&candidates, &web_get("https://x.test/"), device()),
        Decision::Deny
    );
    assert_eq!(
        evaluate(&candidates, &web_get("https://y.test/"), device()),
        Decision::Allow
    );
}

#[test]
fn read_write_covers_read_but_not_the_other_way() {
    let rw = [permission(
        "filesystem",
        "readWrite",
        "/home/u/docs",
        "granted",
        device(),
    )];
    assert_eq!(
        evaluate(&rw, &read_path("/home/u/docs/a"), device()),
        Decision::Allow
    );
    let read = [permission(
        "filesystem",
        "read",
        "/home/u/docs",
        "granted",
        device(),
    )];
    assert_eq!(
        evaluate(&read, &write_path("/home/u/docs/a"), device()),
        Decision::Prompt
    );
}

#[test]
fn a_web_method_must_be_covered() {
    let get_only = [permission("web", "GET", "x.test", "granted", VAULT_WIDE)];
    let mut put = web_get("https://x.test/");
    put.action = Action::Method("PUT".to_string());
    assert_eq!(evaluate(&get_only, &put, device()), Decision::Prompt);
    assert_eq!(
        evaluate(&get_only, &web_get("https://x.test/"), device()),
        Decision::Allow
    );
}

#[test]
fn only_vault_wide_and_this_devices_rows_count() {
    let elsewhere = [permission(
        "filesystem",
        "read",
        "/home/u",
        "granted",
        other_device(),
    )];
    assert_eq!(
        evaluate(&elsewhere, &read_path("/home/u/a"), device()),
        Decision::Prompt
    );
    let here = [permission(
        "filesystem",
        "read",
        "/home/u",
        "granted",
        device(),
    )];
    assert_eq!(
        evaluate(&here, &read_path("/home/u/a"), device()),
        Decision::Allow
    );
    let everywhere = [permission(
        "filesystem",
        "read",
        "/home/u",
        "granted",
        VAULT_WIDE,
    )];
    assert_eq!(
        evaluate(&everywhere, &read_path("/home/u/a"), device()),
        Decision::Allow
    );
}

#[test]
fn a_denial_on_another_device_does_not_count_here() {
    let candidates = [
        permission("filesystem", "read", "/home/u", "denied", other_device()),
        permission("filesystem", "read", "/home/u", "granted", VAULT_WIDE),
    ];
    assert_eq!(
        evaluate(&candidates, &read_path("/home/u/a"), device()),
        Decision::Allow
    );
}

#[test]
fn a_permission_of_another_kind_never_counts() {
    let shell = [permission("shell", "execute", "*", "granted", VAULT_WIDE)];
    assert_eq!(
        evaluate(&shell, &read_path("/home/u/a"), device()),
        Decision::Prompt
    );
}

#[test]
fn unknown_stored_values_are_absent_never_another_permission() {
    // FR-022: haex-vault 2.x (Legacy) fell back to "database read" for unknown values (`crud.rs:246-255`).
    for (kind, action, target, status) in [
        ("spaces", "read", "*", "granted"),
        ("database", "delete", "x", "granted"),
        ("web", "get", "*", "granted"),
        ("filesystem", "read", "relative/path", "granted"),
        ("database", "read", "chat_*", "granted"),
        ("web", "*", "*", "maybe"),
        ("notifications", "send", "*", "granted"),
    ] {
        assert!(
            Permission::from_row(kind, action, target, status, VAULT_WIDE).is_none(),
            "{kind} {action} {target} {status}"
        );
    }
}

#[test]
fn only_the_shell_is_a_device_scoped_kind() {
    for kind in [
        PermissionKind::Database,
        PermissionKind::Filesystem,
        PermissionKind::Web,
        PermissionKind::Notifications,
        PermissionKind::Passwords,
        PermissionKind::RemoteStorage,
        PermissionKind::Mail,
        PermissionKind::Shell,
    ] {
        let expected = matches!(kind, PermissionKind::Shell);
        assert_eq!(kind.is_device_scoped(), expected, "{kind:?}");
        assert_eq!(PermissionKind::parse(kind.as_str()), Some(kind));
    }
}

#[test]
fn the_scope_maps_to_the_device_column() {
    assert_eq!(
        GrantScope::from_device_column(VAULT_WIDE),
        GrantScope::Vault
    );
    assert_eq!(
        GrantScope::from_device_column(device()),
        GrantScope::Device(device())
    );
    assert_eq!(GrantScope::Vault.device_column(), VAULT_WIDE);
    assert_eq!(GrantScope::Device(device()).device_column(), device());
}

#[test]
fn actions_parse_per_kind() {
    assert_eq!(
        Action::parse(PermissionKind::Database, "readWrite"),
        Some(Action::ReadWrite)
    );
    assert_eq!(Action::parse(PermissionKind::Database, "execute"), None);
    assert_eq!(
        Action::parse(PermissionKind::Web, "PROPFIND"),
        Some(Action::Method("PROPFIND".to_string()))
    );
    assert_eq!(
        Action::parse(PermissionKind::Web, "*"),
        Some(Action::AnyMethod)
    );
    assert_eq!(
        Action::parse(PermissionKind::Mail, "poll"),
        Some(Action::Poll)
    );
    assert_eq!(
        Action::parse(PermissionKind::Shell, "execute"),
        Some(Action::Execute)
    );
    assert_eq!(
        Action::parse(PermissionKind::Notifications, "show"),
        Some(Action::Show)
    );
    for action in [
        Action::Read,
        Action::ReadWrite,
        Action::AnyMethod,
        Action::Method("GET".to_string()),
    ] {
        let kind = if matches!(action, Action::Read | Action::ReadWrite) {
            PermissionKind::Database
        } else {
            PermissionKind::Web
        };
        assert_eq!(Action::parse(kind, &action.as_string()), Some(action));
    }
}

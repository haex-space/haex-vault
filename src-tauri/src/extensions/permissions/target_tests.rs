use std::path::PathBuf;

use super::*;
use crate::extensions::ids::ExtensionTable;
use crate::extensions::permissions::Action;

const PK: &str = "abababababababababababababababababababababababababababababababab";
const OTHER: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";

fn table(name: &str) -> RequestTarget {
    RequestTarget::Table(ExtensionTable::parse(name).expect("extension table"))
}

fn url(value: &str) -> RequestTarget {
    RequestTarget::Url(WebRequest::parse(value).expect("url"))
}

fn matches(kind: PermissionKind, target: &str, request: &RequestTarget) -> bool {
    Target::parse(kind, target)
        .unwrap_or_else(|| panic!("{target} parses for {kind:?}"))
        .matches(request)
}

#[test]
fn a_database_target_is_an_extension_prefix_or_one_of_its_tables() {
    let prefix = format!("{PK}__calendar__*");
    assert!(matches(
        PermissionKind::Database,
        &prefix,
        &table(&format!("{PK}__calendar__events"))
    ));
    assert!(!matches(
        PermissionKind::Database,
        &prefix,
        &table(&format!("{OTHER}__calendar__events"))
    ));
    assert!(!matches(
        PermissionKind::Database,
        &prefix,
        &table(&format!("{PK}__calendar-pro__events"))
    ));

    let one = format!("{PK}__calendar__events");
    assert!(matches(
        PermissionKind::Database,
        &one,
        &table(&format!("{PK}__calendar__events"))
    ));
    assert!(!matches(
        PermissionKind::Database,
        &one,
        &table(&format!("{PK}__calendar__attendees"))
    ));
}

#[test]
fn a_database_target_never_names_a_core_table() {
    for bad in [
        "*",
        "chat_*",
        "chat_threads",
        "haex_*",
        "sqlite_master",
        "extensions",
    ] {
        assert!(
            Target::parse(PermissionKind::Database, bad).is_none(),
            "{bad}"
        );
    }
    assert!(Target::parse(PermissionKind::Database, &format!("{PK}__a__b__*")).is_none());
}

#[test]
fn a_path_target_matches_whole_path_components() {
    let docs = RequestTarget::Path(PathBuf::from("/home/u/docs/a.txt"));
    let private = RequestTarget::Path(PathBuf::from("/home/u/docs-private/a.txt"));
    assert!(matches(PermissionKind::Filesystem, "/home/u/docs", &docs));
    assert!(matches(PermissionKind::Filesystem, "/home/u/docs/*", &docs));
    assert!(!matches(
        PermissionKind::Filesystem,
        "/home/u/docs",
        &private
    ));
    assert!(matches(
        PermissionKind::Filesystem,
        "/home/u/docs/a.txt",
        &docs
    ));
    assert!(matches(PermissionKind::Filesystem, "*", &private));
    assert!(Target::parse(PermissionKind::Filesystem, "docs").is_none());
    assert!(Target::parse(PermissionKind::Filesystem, "/home/u/../etc").is_none());
}

#[test]
fn a_bare_domain_matches_itself_and_subdomains_at_a_label_boundary() {
    assert!(matches(
        PermissionKind::Web,
        "example.org",
        &url("https://example.org/x")
    ));
    assert!(matches(
        PermissionKind::Web,
        "example.org",
        &url("https://dav.example.org/x")
    ));
    assert!(!matches(
        PermissionKind::Web,
        "example.org",
        &url("https://badexample.org/x")
    ));
    assert!(matches(
        PermissionKind::Web,
        "*.example.org",
        &url("https://dav.example.org/")
    ));
    assert!(!matches(
        PermissionKind::Web,
        "*.example.org",
        &url("https://example.org/")
    ));
    assert!(matches(
        PermissionKind::Web,
        "EXAMPLE.org",
        &url("https://Example.ORG/")
    ));
}

#[test]
fn a_url_pattern_matches_scheme_host_port_and_path() {
    let pattern = "https://dav.example.org/cal/*";
    assert!(matches(
        PermissionKind::Web,
        pattern,
        &url("https://dav.example.org/cal/1.ics")
    ));
    assert!(!matches(
        PermissionKind::Web,
        pattern,
        &url("http://dav.example.org/cal/1.ics")
    ));
    assert!(!matches(
        PermissionKind::Web,
        pattern,
        &url("https://dav.example.org/other")
    ));
    assert!(!matches(
        PermissionKind::Web,
        pattern,
        &url("https://dav.example.org:8443/cal/1.ics")
    ));
    assert!(matches(
        PermissionKind::Web,
        "https://dav.example.org:8443/cal/*",
        &url("https://dav.example.org:8443/cal/1.ics")
    ));
    assert!(matches(
        PermissionKind::Web,
        "https://*.example.org/*",
        &url("https://a.example.org/z")
    ));
    assert!(!matches(
        PermissionKind::Web,
        "https://dav.example.org/cal",
        &url("https://dav.example.org/cal/1.ics")
    ));
    assert!(matches(PermissionKind::Web, "*", &url("http://x.test/")));
}

#[test]
fn a_host_with_a_trailing_dot_is_the_same_host() {
    // `example.org.` reaches the same server as `example.org`; a denial must not miss it.
    assert!(matches(
        PermissionKind::Web,
        "example.org",
        &url("https://example.org./x")
    ));
    assert!(matches(
        PermissionKind::Web,
        "*.example.org",
        &url("https://dav.example.org./")
    ));
    assert!(matches(
        PermissionKind::Web,
        "https://dav.example.org/cal/*",
        &url("https://dav.example.org./cal/1.ics")
    ));
    let imap = RequestTarget::MailServer {
        host: "imap.example.org.".to_string(),
        port: 993,
    };
    assert!(matches(PermissionKind::Mail, "imap.example.org", &imap));
}

#[test]
fn only_http_and_https_are_web_requests() {
    assert!(WebRequest::parse("file:///etc/passwd").is_none());
    assert!(WebRequest::parse("ftp://x.test/").is_none());
    assert!(WebRequest::parse("not a url").is_none());
}

#[test]
fn a_mail_target_names_a_host_and_optionally_a_port() {
    let imap = RequestTarget::MailServer {
        host: "imap.example.org".to_string(),
        port: 993,
    };
    assert!(matches(PermissionKind::Mail, "imap.example.org:993", &imap));
    assert!(matches(PermissionKind::Mail, "IMAP.example.org", &imap));
    assert!(!matches(
        PermissionKind::Mail,
        "imap.example.org:143",
        &imap
    ));
    assert!(!matches(PermissionKind::Mail, "example.org", &imap));
    assert!(
        matches(PermissionKind::Mail, "*", &imap),
        "every server, as haex-mail declares it"
    );
}

#[test]
fn a_shell_target_is_an_exact_program_or_everything() {
    let sh = RequestTarget::Program(PathBuf::from("/bin/sh"));
    assert!(matches(PermissionKind::Shell, "/bin/sh", &sh));
    assert!(!matches(PermissionKind::Shell, "/bin", &sh));
    assert!(matches(PermissionKind::Shell, "*", &sh));
    assert!(Target::parse(PermissionKind::Shell, "sh").is_none());
}

#[test]
fn tags_storage_ids_and_notifications() {
    let tag = RequestTarget::Tag("Haex-Calendar".to_string());
    assert!(matches(PermissionKind::Passwords, "haex-calendar", &tag));
    assert!(matches(PermissionKind::Passwords, "*", &tag));
    assert!(!matches(PermissionKind::Passwords, "s3", &tag));

    let storage = RequestTarget::StorageId("conn-1".to_string());
    assert!(matches(PermissionKind::RemoteStorage, "conn-1", &storage));
    assert!(!matches(PermissionKind::RemoteStorage, "conn-2", &storage));

    assert!(matches(
        PermissionKind::Notifications,
        "*",
        &RequestTarget::Any
    ));
    assert!(Target::parse(PermissionKind::Notifications, "x").is_none());
}

#[test]
fn the_host_of_a_proposed_storage_endpoint_matches_like_a_mail_server() {
    let endpoint = RequestTarget::Endpoint {
        host: "NAS.local".to_string(),
        port: 9000,
    };
    for target in ["nas.local:9000", "nas.local", "*"] {
        assert!(
            matches(PermissionKind::RemoteStorage, target, &endpoint),
            "{target}"
        );
    }
    for target in ["nas.local:9001", "other.local", "evilnas.local"] {
        assert!(
            !matches(PermissionKind::RemoteStorage, target, &endpoint),
            "{target}"
        );
    }
    assert!(
        !matches(
            PermissionKind::RemoteStorage,
            "nas.local",
            &RequestTarget::StorageId("conn-1".to_string())
        ),
        "a host is not a storage"
    );
}

#[test]
fn an_ipv6_endpoint_matches_a_grant_in_brackets() {
    let endpoint = RequestTarget::Endpoint {
        host: "[fd00::1]".to_string(),
        port: 9000,
    };
    for target in ["[fd00::1]:9000", "[FD00:0:0::1]", "[fd00::1]", "*"] {
        assert!(
            matches(PermissionKind::RemoteStorage, target, &endpoint),
            "{target}"
        );
    }
    for target in [
        "[fd00::1]:9001",
        "[fd00::2]",
        "fd00::1",
        "[fd00::1",
        "[fd00::1]9000",
        "[nas.local]",
    ] {
        assert!(
            !matches(PermissionKind::RemoteStorage, target, &endpoint),
            "{target}"
        );
    }
}

#[test]
fn add_is_an_action_of_remote_storage_that_read_write_does_not_cover() {
    let add = Action::parse(PermissionKind::RemoteStorage, "add").expect("add");
    assert_eq!(add.as_string(), "add");
    assert!(!Action::ReadWrite.covers(&add));
    assert!(Action::parse(PermissionKind::Passwords, "add").is_none());
}

#[test]
fn an_encrypted_folder_target_names_exactly_one_folder() {
    let folder = "s1/aaaaaaaaaaaaaaaaaaaaaaaaaa";
    let request = RequestTarget::EncryptedFolder(FolderRef::parse(folder).unwrap());
    assert!(matches(PermissionKind::EncryptedFolder, folder, &request));
    let other =
        RequestTarget::EncryptedFolder(FolderRef::parse("s1/baaaaaaaaaaaaaaaaaaaaaaaaa").unwrap());
    assert!(!matches(PermissionKind::EncryptedFolder, folder, &other));
    for bad in [
        "*",
        "s1",
        "s1/*",
        "/aaaaaaaaaaaaaaaaaaaaaaaaaa",
        "s1/AAAAAAAAAAAAAAAAAAAAAAAAAA",
        "s1/x",
    ] {
        assert!(
            Target::parse(PermissionKind::EncryptedFolder, bad).is_none(),
            "{bad}"
        );
    }
    // `*` of another kind never reaches an encrypted folder.
    assert!(!Target::Any.matches(&request));
    assert_eq!(
        Action::parse(PermissionKind::EncryptedFolder, "readWrite"),
        Some(Action::ReadWrite)
    );
    assert_eq!(Action::parse(PermissionKind::EncryptedFolder, "add"), None);
}

/// Checks tool targets accept valid names and `*`, rejecting other spellings.
#[test]
fn an_agent_tool_permission_names_a_tool_or_every_tool() {
    let kind = PermissionKind::AgentTool;
    assert_eq!(PermissionKind::parse("agentTool"), Some(kind));
    assert!(!kind.is_device_scoped());
    assert_eq!(
        Target::parse(kind, "count_unread"),
        Some(Target::Tool("count_unread".into()))
    );
    assert_eq!(Target::parse(kind, "*"), Some(Target::Any));
    assert_eq!(Target::parse(kind, "Count-Unread"), None);
    assert_eq!(Target::parse(kind, ""), None);
}

/// Checks tool actions accept effects and wildcard permissions cover every effect.
#[test]
fn an_agent_tool_permission_has_an_effect_and_the_star_row_covers_every_effect() {
    let kind = PermissionKind::AgentTool;
    for effect in ["read", "change", "destructive", "*"] {
        assert_eq!(Action::parse(kind, effect).unwrap().as_string(), effect);
    }
    assert_eq!(Action::parse(kind, "readWrite"), None);
    let every = Action::parse(kind, "*").unwrap();
    for effect in [Action::Read, Action::Change, Action::Destructive] {
        assert!(every.covers(&effect));
    }
    assert!(!Action::Change.covers(&Action::Destructive));
}

#[test]
fn a_contacts_target_is_all_or_a_group_by_its_folded_name() {
    use crate::extensions::permissions::Permission;
    assert_eq!(
        Target::parse(PermissionKind::Contacts, "*"),
        Some(Target::Any)
    );
    assert_eq!(
        Target::parse(PermissionKind::Contacts, " Arbeit "),
        Some(Target::Tag("arbeit".to_string()))
    );
    assert_eq!(Target::parse(PermissionKind::Contacts, "  "), None);
    assert_eq!(
        PermissionKind::parse("contacts"),
        Some(PermissionKind::Contacts)
    );
    assert_eq!(
        Action::parse(PermissionKind::Contacts, "read"),
        Some(Action::Read)
    );
    for refused in ["readWrite", "write", "*"] {
        assert_eq!(Action::parse(PermissionKind::Contacts, refused), None);
    }
    assert!(Permission::from_row(
        "contacts",
        "readWrite",
        "*",
        "granted",
        crate::extensions::permissions::VAULT_WIDE
    )
    .is_none());
    assert!(!PermissionKind::Contacts.is_device_scoped());
}

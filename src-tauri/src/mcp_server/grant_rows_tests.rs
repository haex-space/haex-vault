use uuid::Uuid;

use super::grant_rows::{rights_of, rows_of, GrantRow};
use super::grants::{
    default_rights, AccessLevel, BrowserRights, ContactsRights, DelegateRights, FilesRights,
    FolderRight, OperateLevel, PasswordLevel, PasswordRights, RequestRule, Rights, SiteGrant,
    SiteLevel,
};
use crate::adapters::cli_delegate::autonomy::{AutonomyMode, DenyCategory};
use crate::agent_browser::sites::Site;
use crate::files::permissions::AgentFileStatus;

fn site(text: &str) -> Site {
    Site::parse(text).expect("site")
}

fn row(group: &str, key: &str, level: &str, tags: Option<&str>) -> GrantRow {
    GrantRow {
        group: group.to_owned(),
        key: key.to_owned(),
        level: level.to_owned(),
        tags: tags.map(str::to_owned),
    }
}

fn every_group() -> Rights {
    Rights {
        operate: [("settings.device".to_owned(), OperateLevel::Write)]
            .into_iter()
            .collect(),
        browser: Some(BrowserRights {
            user_browser: true,
            sites: [
                (
                    site("https://shop.example.de"),
                    SiteGrant {
                        level: SiteLevel::Operate,
                        fill_remembered: true,
                    },
                ),
                (
                    site("http://intranet.example"),
                    SiteGrant {
                        level: SiteLevel::Read,
                        fill_remembered: false,
                    },
                ),
            ]
            .into_iter()
            .collect(),
            requests: [
                (site("https://tracker.example"), RequestRule::Deny),
                (site("https://api.shop.example.de"), RequestRule::Allow),
            ]
            .into_iter()
            .collect(),
            all: Some(SiteLevel::Read),
        }),
        passwords: Some(PasswordRights {
            level: PasswordLevel::Read,
            tags: Some(vec!["dev".to_owned(), "Büro".to_owned()]),
            default_tag: None,
        }),
        extensions: [Uuid::from_u128(0x4d41_494c)].into_iter().collect(),
        all_extensions: true,
        commands: true,
        delegate: Some(DelegateRights {
            autonomy: AutonomyMode::GatedPermissive,
            deny: [DenyCategory::NetworkAccess, DenyCategory::CredentialPaths]
                .into_iter()
                .collect(),
        }),
        contacts: ContactsRights {
            all: Some(AccessLevel::Read),
            groups: [
                ("g1".to_owned(), AccessLevel::ReadWrite),
                ("g2".to_owned(), AccessLevel::Denied),
            ]
            .into_iter()
            .collect(),
        },
        files: FilesRights {
            device: true,
            storages: [("s1".to_owned(), AccessLevel::ReadWrite)]
                .into_iter()
                .collect(),
            folders: [
                (
                    "s1/f1".to_owned(),
                    FolderRight {
                        level: AccessLevel::Read,
                        cloud: true,
                    },
                ),
                (
                    "s1/f2".to_owned(),
                    FolderRight {
                        level: AccessLevel::Denied,
                        cloud: false,
                    },
                ),
            ]
            .into_iter()
            .collect(),
            file_shares: true,
            sync_rules: true,
        },
    }
}

#[test]
fn the_contacts_group_is_a_row_for_all_and_a_row_per_group() {
    let rows = rows_of(&every_group());
    assert!(rows.contains(&row("contacts", "*", "read", None)));
    assert!(rows.contains(&row("contacts", "group:g1", "readWrite", None)));
    assert!(rows.contains(&row("contacts", "group:g2", "denied", None)));
    assert_eq!(rights_of(rows).contacts, every_group().contacts);
}

#[test]
fn a_contacts_row_this_build_cannot_read_never_gives_more() {
    // A level from a newer build is a refusal, not a skipped row.
    let contacts = rights_of([
        row("contacts", "*", "read", None),
        row("contacts", "group:g1", "from_the_future", None),
    ])
    .contacts;
    assert_eq!(contacts.all, Some(AccessLevel::Read));
    assert_eq!(contacts.groups["g1"], AccessLevel::Denied);
    // A key that is neither all nor a group is no right.
    let contacts = rights_of([
        row("contacts", "group:", "read", None),
        row("contacts", "somebody", "readWrite", None),
    ])
    .contacts;
    assert!(contacts.is_empty());
    // The address book reads the entries as it read the rows of before.
    assert_eq!(
        every_group().contacts.rows(),
        [
            ("*".to_owned(), AgentFileStatus::Read),
            ("group:g1".to_owned(), AgentFileStatus::ReadWrite),
            ("group:g2".to_owned(), AgentFileStatus::Denied),
        ]
    );
}

#[test]
fn the_delegate_group_is_stored_as_an_autonomy_row_and_a_row_per_deny_category() {
    let rows = rows_of(&every_group());
    assert!(rows.contains(&row("delegate", "autonomy", "gated_permissive", None)));
    assert!(rows.contains(&row("delegate", "deny:network_access", "on", None)));
    assert!(rows.contains(&row("delegate", "deny:credential_paths", "on", None)));
    assert!(!rows.iter().any(|r| r.key == "deny:workspace_escape"));
}

#[test]
fn a_delegate_group_this_build_cannot_read_is_off_and_never_gives_more() {
    // No autonomy row or an unknown autonomy: the group is off.
    assert_eq!(
        rights_of([row("delegate", "deny:network_access", "on", None)]).delegate,
        None,
        "deny rows without an autonomy row are no group"
    );
    assert_eq!(
        rights_of([row("delegate", "autonomy", "anything_goes", None)]).delegate,
        None
    );
    // A category from a newer build would refuse something this build cannot: skipping it would
    // refuse less than was set, so the whole group is off and every call asks.
    assert_eq!(
        rights_of([
            row("delegate", "autonomy", "gated_permissive", None),
            row("delegate", "deny:from_the_future", "on", None),
        ])
        .delegate,
        None
    );
    let read = rights_of([
        row("delegate", "autonomy", "gated_permissive", None),
        row("delegate", "deny:workspace_escape", "off", None),
    ])
    .delegate
    .expect("a group");
    assert_eq!(read.autonomy, AutonomyMode::GatedPermissive);
    assert!(read.deny.is_empty(), "a switched-off row is no rule");
}

#[test]
fn every_group_round_trips_through_its_rows() {
    for rights in [every_group(), default_rights(), Rights::default()] {
        assert_eq!(rights_of(rows_of(&rights)), rights);
    }
    let rows = rows_of(&every_group());
    assert!(rows.contains(&row("browser", "", "on", None)));
    assert!(rows.contains(&row(
        "browser",
        "https://shop.example.de",
        "operate",
        Some(r#"["fill"]"#)
    )));
    assert!(rows.contains(&row(
        "browser",
        "request:https://tracker.example",
        "deny",
        None
    )));
    assert!(rows.contains(&row(
        "browser",
        "request:https://api.shop.example.de",
        "allow",
        None
    )));
    assert!(rows.contains(&row("browser", "*", "read", None)));
    assert!(rows.contains(&row("passwords", "", "read", Some(r#"["dev","Büro"]"#))));
    assert!(rows.contains(&row("commands", "", "on", None)));
}

#[test]
fn a_group_that_is_off_has_no_rows() {
    assert!(rows_of(&Rights::default()).is_empty());
    // The factory rights are "holzi bedienen" at read and the autonomy of today's default.
    let rows = rows_of(&default_rights());
    assert!(rows
        .iter()
        .all(|row| row.group == "operate" || row.group == "delegate"));
    assert!(rows.contains(&row("delegate", "autonomy", "ungated", None)));
}

#[test]
fn the_browser_group_is_off_without_its_switch_row() {
    let without_switch = rows_of(&every_group())
        .into_iter()
        .filter(|row| !(row.group == "browser" && row.key.is_empty()));
    assert_eq!(rights_of(without_switch).browser, None);
}

/// FR-008: a row a newer build or another device wrote in a shape this one cannot read opens
/// nothing, and never widens a group.
#[test]
fn a_row_this_build_cannot_read_is_no_right() {
    let rights = rights_of([
        row("operate", "wm.read", "everything", None),
        row("passwords", "", "read", Some("not json")),
        row("extension", "not-a-uuid", "on", None),
        row("extension", &Uuid::from_u128(1).to_string(), "later", None),
        row("commands", "", "later", None),
        row("browser", "", "on", None),
        row("browser", "https://shop.example.de", "drive", None),
        row("browser", "not a site", "read", None),
        row("browser", "*", "everything", None),
        row("browser", "request:not a site", "allow", None),
        row("browser", "request:https://shop.example.de", "maybe", None),
        row("a-later-group", "", "on", None),
    ]);
    assert_eq!(
        rights,
        Rights {
            browser: Some(BrowserRights::default()),
            ..Rights::default()
        }
    );
}

#[test]
fn a_fill_tag_counts_only_on_an_operated_website() {
    let rights = rights_of([
        row("browser", "", "on", None),
        row(
            "browser",
            "https://shop.example.de",
            "read",
            Some(r#"["fill"]"#),
        ),
    ]);
    let sites = rights.browser.expect("on").sites;
    assert!(!sites[&site("https://shop.example.de")].fill_remembered);
}

/// Spec 065 FR-036: writing passwords keeps the tag new entries get in the row's key.
#[test]
fn writing_passwords_keeps_the_tag_for_new_entries() {
    let rights = Rights {
        passwords: Some(PasswordRights {
            level: PasswordLevel::ReadWrite,
            tags: Some(vec!["dev".to_owned(), "shop".to_owned()]),
            default_tag: Some("shop".to_owned()),
        }),
        ..Rights::default()
    };
    let rows = rows_of(&rights);
    assert!(rows.contains(&row(
        "passwords",
        "shop",
        "read_write",
        Some(r#"["dev","shop"]"#)
    )));
    assert_eq!(rights_of(rows), rights);
}

/// FR-047: a blocked website is stored in its row; a blocked level for all websites is no right.
#[test]
fn a_blocked_website_is_stored_and_a_blocked_level_for_all_reads_as_nothing() {
    let rights = Rights {
        browser: Some(BrowserRights {
            all: Some(SiteLevel::Operate),
            sites: [(
                site("https://bank.example"),
                SiteGrant {
                    level: SiteLevel::Blocked,
                    fill_remembered: false,
                },
            )]
            .into_iter()
            .collect(),
            ..BrowserRights::default()
        }),
        ..Rights::default()
    };
    let rows = rows_of(&rights);
    assert!(rows.contains(&row("browser", "https://bank.example", "blocked", None)));
    assert_eq!(rights_of(rows), rights);
    let stored = rights_of([
        row("browser", "", "on", None),
        row("browser", "*", "blocked", None),
    ]);
    assert_eq!(stored.browser.expect("on").all, None);
}

/// FR-043: a group stored before the level for all websites existed has none; a new group
/// starts with operating.
#[test]
fn only_a_group_switched_on_from_now_on_may_operate_every_website() {
    let stored = rights_of([row("browser", "", "on", None)]);
    assert_eq!(stored.browser.expect("on").all, None);
    assert_eq!(BrowserRights::switched_on().all, Some(SiteLevel::Operate));
    let rows = rows_of(&Rights {
        browser: Some(BrowserRights::switched_on()),
        ..Rights::default()
    });
    assert!(rows.contains(&row("browser", "*", "operate", None)));
}

/// FR-012 (spec 083 T008): a row whose key this build does not know is no right, so a profile or an
/// old row never widens an agent through a key that only looks like one.
#[test]
fn a_row_with_a_key_this_build_does_not_know_is_no_right() {
    let rights = rights_of([
        row("commands", "shell", "on", None),
        row("operate", "files.shares", "write", None),
        row("operate", "settings.guardrails", "write", None),
        row("operate", "wm.read", "read", None),
        row("commands", "", "on", None),
    ]);
    assert!(rights.commands, "the commands row has the empty key");
    assert_eq!(
        rights.operate,
        [("wm.read".to_owned(), OperateLevel::Read)]
            .into_iter()
            .collect(),
        "only the scopes of holzi bedienen"
    );
    assert!(!rights_of([row("commands", "shell", "on", None)]).commands);
}

#[test]
fn the_row_for_all_extensions_has_the_key_star_and_is_no_extension() {
    let rights = Rights {
        all_extensions: true,
        ..Rights::default()
    };
    let rows = rows_of(&rights);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].group.as_str(), rows[0].key.as_str()),
        ("extension", "*")
    );
    assert_eq!(rights_of(rows), rights);
    // Off, or at any other level, it is no right.
    let off = GrantRow {
        group: "extension".to_owned(),
        key: "*".to_owned(),
        level: "off".to_owned(),
        tags: None,
    };
    assert_eq!(rights_of([off]), Rights::default());
}

#[test]
fn the_files_group_is_switch_rows_and_a_row_per_storage_and_folder() {
    let rows = rows_of(&every_group());
    for expected in [
        row("files", "device", "on", None),
        row("files", "shares", "on", None),
        row("files", "sync", "on", None),
        row("files", "storage:s1", "readWrite", None),
        row("files", "folder:s1/f1", "read", Some(r#"["cloud"]"#)),
        row("files", "folder:s1/f2", "denied", None),
    ] {
        assert!(rows.contains(&expected), "{expected:?}");
    }
    assert_eq!(rights_of(rows).files, every_group().files);
}

#[test]
fn a_files_row_this_build_cannot_read_never_gives_more() {
    let files = rights_of([
        row("files", "storage:s1", "from_the_future", None),
        row(
            "files",
            "folder:s1/f1",
            "from_the_future",
            Some(r#"["cloud"]"#),
        ),
        row("files", "somebody", "on", None),
        row("files", "storage:", "read", None),
        row("files", "folder:no-slash", "read", None),
    ])
    .files;
    assert_eq!(files.storages["s1"], AccessLevel::Denied);
    assert_eq!(files.folders["s1/f1"].level, AccessLevel::Denied);
    assert_eq!(
        files.storages.len(),
        1,
        "keys that are no right are skipped"
    );
    assert_eq!(files.folders.len(), 1);
    assert!(!files.device && !files.file_shares && !files.sync_rules);
}

#[test]
fn the_files_rights_become_the_grants_the_checks_read() {
    use crate::files::access::{DeviceGrant, Locality, StorageGrant};
    let grants = every_group().files.grants();
    assert_eq!(grants.device, Some(DeviceGrant::Granted));
    assert_eq!(grants.storages["s1"], StorageGrant::ReadWrite);
    assert_eq!(grants.folders["s1/f1"].reach, Locality::Cloud);
    assert_eq!(grants.folders["s1/f2"].reach, Locality::Local);
    assert!(grants.file_shares && grants.sync_rules);
    // Nothing granted: the device is refused for every agent, the chat's own included.
    let none = FilesRights::default().grants();
    assert_eq!(none.device, Some(DeviceGrant::Denied));
}

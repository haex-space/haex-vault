use uuid::Uuid;

use super::grant_rows::{rights_of, rows_of, GrantRow};
use super::grants::{
    default_rights, BrowserRights, OperateLevel, PasswordLevel, PasswordRights, Rights, SiteGrant,
    SiteLevel,
};
use crate::agent_browser::sites::Site;

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
        }),
        passwords: Some(PasswordRights {
            level: PasswordLevel::Read,
            tags: Some(vec!["dev".to_owned(), "Büro".to_owned()]),
        }),
        extensions: [Uuid::from_u128(0x4d41_494c)].into_iter().collect(),
        commands: true,
    }
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
    assert!(rows.contains(&row("passwords", "", "read", Some(r#"["dev","Büro"]"#))));
    assert!(rows.contains(&row("commands", "", "on", None)));
}

#[test]
fn a_group_that_is_off_has_no_rows() {
    assert!(rows_of(&Rights::default()).is_empty());
    let rows = rows_of(&default_rights());
    assert!(rows.iter().all(|row| row.group == "operate"));
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

use url::Url;

use super::act::new_login::{new_entry, tags_for_new_entry};
use crate::mcp_server::grants::{validate, InvalidRights, PasswordLevel, PasswordRights, Rights};

fn rights(
    level: PasswordLevel,
    tags: Option<&[&str]>,
    default_tag: Option<&str>,
) -> PasswordRights {
    PasswordRights {
        level,
        tags: tags.map(|tags| tags.iter().map(|tag| (*tag).to_owned()).collect()),
        default_tag: default_tag.map(str::to_owned),
    }
}

#[test]
fn a_new_entry_gets_the_tag_chosen_for_new_entries() {
    let shop = rights(
        PasswordLevel::ReadWrite,
        Some(&["dev", "shop"]),
        Some("shop"),
    );
    assert_eq!(tags_for_new_entry(&shop), vec!["shop".to_owned()]);
    let all = rights(PasswordLevel::ReadWrite, None, None);
    assert!(tags_for_new_entry(&all).is_empty());
}

#[test]
fn a_new_entry_holds_the_website_the_username_and_the_password() {
    let page = Url::parse("https://shop.example:8443/konto/neu?ref=1").unwrap();
    let entry = new_entry(&page, "anna@example.org", "geheim", vec!["shop".to_owned()]);
    assert_eq!(entry.title.as_deref(), Some("shop.example"));
    assert_eq!(entry.url.as_deref(), Some("https://shop.example:8443"));
    assert_eq!(entry.username.as_deref(), Some("anna@example.org"));
    assert_eq!(entry.password.as_deref(), Some("geheim"));
    assert_eq!(entry.tags, vec!["shop".to_owned()]);
}

#[test]
fn writing_within_chosen_tags_needs_one_of_them_for_new_entries() {
    let with = |passwords| Rights {
        passwords: Some(passwords),
        ..Rights::default()
    };
    for missing in [None, Some("other")] {
        assert_eq!(
            validate(&with(rights(
                PasswordLevel::ReadWrite,
                Some(&["dev"]),
                missing
            ))),
            Err(InvalidRights::DefaultTag)
        );
    }
    assert!(validate(&with(rights(
        PasswordLevel::ReadWrite,
        Some(&["dev"]),
        Some("dev")
    )))
    .is_ok());
    assert!(validate(&with(rights(PasswordLevel::ReadWrite, None, None))).is_ok());
    assert!(validate(&with(rights(PasswordLevel::Read, Some(&["dev"]), None))).is_ok());
}

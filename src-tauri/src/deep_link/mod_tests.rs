use super::*;

#[test]
fn only_a_known_link_of_the_scheme_is_a_share() {
    assert!(known_link("haex-vault://contact/AAAA").is_some());
    for other in [
        "",
        "haex-vault://contact/",
        "haex-vault://other/AAAA",
        "haexvault://contact/AAAA",
        "https://example.com/haex-vault://contact/AAAA",
        "holzi:card:AAAA",
        "tel:+491701234567",
        "https://example.com/mailto:ben@example.com",
    ] {
        assert!(known_link(other).is_none(), "{other}");
    }
}

#[test]
fn a_link_longer_than_a_share_can_be_is_not_one() {
    let long = format!("{LINK_PREFIX}{}", "A".repeat(MAX_LINK_LEN));
    assert!(known_link(&long).is_none());
    let fits = format!(
        "{LINK_PREFIX}{}",
        "A".repeat(MAX_LINK_LEN - LINK_PREFIX.len())
    );
    assert!(known_link(&fits).is_some());
}

#[test]
fn the_first_known_link_of_the_arguments_is_taken_and_the_rest_ignored() {
    let urls = [
        "haex-vault://settings/x".to_string(),
        "https://example.com".to_string(),
        "haex-vault://contact/ONE".to_string(),
        "haex-vault://contact/TWO".to_string(),
    ];
    assert_eq!(
        first_known_link(urls).as_deref(),
        Some("haex-vault://contact/ONE")
    );
    assert_eq!(
        first_known_link(["mailto:x@example.com".to_string()]).as_deref(),
        Some("mailto:x@example.com")
    );
    assert_eq!(
        first_known_link(["haex-vault://settings/x".to_string()]),
        None
    );
}

#[test]
fn a_mail_link_is_known_in_any_case_and_up_to_its_own_limit() {
    for link in [
        "mailto:ben@example.org",
        "MAILTO:ben@example.org",
        "MailTo:?subject=x",
    ] {
        assert!(known_link(link).is_some(), "{link}");
    }
    let fits = format!(
        "mailto:{}",
        "a".repeat(mail::MAX_LINK_BYTES - mail::SCHEME.len())
    );
    assert!(known_link(&fits).is_some());
    assert!(known_link(&format!("{fits}a")).is_none());
}

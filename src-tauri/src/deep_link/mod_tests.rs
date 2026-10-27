use super::*;

#[test]
fn only_a_contact_link_of_the_scheme_is_a_share() {
    assert!(contact_link("haex-vault://contact/AAAA").is_some());
    for other in [
        "",
        "haex-vault://contact/",
        "haex-vault://other/AAAA",
        "haexvault://contact/AAAA",
        "https://example.com/haex-vault://contact/AAAA",
        "mailto:ben@example.com",
        "holzi:card:AAAA",
    ] {
        assert!(contact_link(other).is_none(), "{other}");
    }
}

#[test]
fn a_link_longer_than_a_share_can_be_is_not_one() {
    let long = format!("{LINK_PREFIX}{}", "A".repeat(MAX_LINK_LEN));
    assert!(contact_link(&long).is_none());
    let fits = format!(
        "{LINK_PREFIX}{}",
        "A".repeat(MAX_LINK_LEN - LINK_PREFIX.len())
    );
    assert!(contact_link(&fits).is_some());
}

#[test]
fn the_first_contact_link_of_the_arguments_is_taken_and_the_rest_ignored() {
    let urls = [
        "mailto:x@example.com".to_string(),
        "haex-vault://settings/x".to_string(),
        "haex-vault://contact/ONE".to_string(),
        "haex-vault://contact/TWO".to_string(),
    ];
    assert_eq!(
        first_contact_link(urls).as_deref(),
        Some("haex-vault://contact/ONE")
    );
    assert_eq!(
        first_contact_link(["haex-vault://settings/x".to_string()]),
        None
    );
}

use super::*;

const VAULT: [u8; 32] = [7; 32];

fn head() -> Head {
    Head {
        display_name: None,
        given_name: None,
        family_name: None,
        middle_name: None,
        name_prefix: None,
        name_suffix: None,
        nickname: None,
        organization: None,
        job_title: None,
        note: None,
    }
}

#[test]
fn the_name_parts_joined_by_spaces() {
    let head = Head {
        name_prefix: Some("Dr.".into()),
        given_name: Some("Ada".into()),
        middle_name: Some("King".into()),
        family_name: Some("Lovelace".into()),
        name_suffix: Some("Jr.".into()),
        ..head()
    };
    let (name, parts) = name_for(&head, Some(VAULT));
    assert_eq!(name, "Dr. Ada King Lovelace Jr.");
    assert_eq!(parts.given.as_deref(), Some("Ada"));
    assert_eq!(parts.suffix.as_deref(), Some("Jr."));
}

#[test]
fn parts_that_are_blank_are_left_out() {
    let head = Head {
        given_name: Some("  Ada ".into()),
        family_name: Some("   ".into()),
        ..head()
    };
    let (name, parts) = name_for(&head, None);
    assert_eq!(name, "Ada");
    assert_eq!(parts.family, None);
}

#[test]
fn without_name_parts_the_npub_of_the_vault() {
    let (name, parts) = name_for(&head(), Some(VAULT));
    assert_eq!(name, received::npub(&VAULT));
    assert!(name.starts_with("npub1"));
    assert_eq!(parts, NameParts::default());
}

#[test]
fn without_name_parts_and_without_a_vault_nothing() {
    assert_eq!(name_for(&head(), None).0, "");
}

#[test]
fn nickname_organization_and_display_name_are_no_substitute() {
    let head = Head {
        display_name: Some("Ada".into()),
        nickname: Some("Countess".into()),
        organization: Some("Analytical Engines".into()),
        ..head()
    };
    assert_eq!(name_for(&head, None).0, "");
    assert_eq!(name_for(&head, Some(VAULT)).0, received::npub(&VAULT));
}

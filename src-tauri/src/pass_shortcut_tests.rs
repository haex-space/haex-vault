use super::*;

const ID: &str = "3f2b8c1e-9a44-4d6b-8f10-5c7e2d9a1b60";

#[test]
fn a_uuid_is_the_id_of_a_pass_and_nothing_else_is() {
    assert_eq!(pass_id(ID).as_deref(), Some(ID));
    assert_eq!(pass_id(&ID.to_uppercase()).as_deref(), Some(ID));
    for other in [
        "",
        "pass-1",
        "3f2b8c1e9a444d6b8f105c7e2d9a1b60",
        "{3f2b8c1e-9a44-4d6b-8f10-5c7e2d9a1b60}",
        "urn:uuid:3f2b8c1e-9a44-4d6b-8f10-5c7e2d9a1b60",
        " 3f2b8c1e-9a44-4d6b-8f10-5c7e2d9a1b60",
        "3f2b8c1e-9a44-4d6b-8f10-5c7e2d9a1b60/",
        "3f2b8c1e-9a44-4d6b-8f10-5c7e2d9a1b60?x=1",
        "../3f2b8c1e-9a44-4d6b-8f10-5c7e2d9a1b60",
    ] {
        assert_eq!(pass_id(other), None, "{other}");
    }
}

#[test]
fn a_link_to_a_pass_names_its_id_and_nothing_more() {
    assert_eq!(
        id_of_link(&format!("{PASS_LINK_PREFIX}{ID}")).as_deref(),
        Some(ID)
    );
    for other in [
        "haex-vault://pass/",
        "haex-vault://pass/not-an-id",
        &format!("haex-vault://contact/{ID}"),
        &format!("haexvault://pass/{ID}"),
        &format!("https://example.com/haex-vault://pass/{ID}"),
        &format!("haex-vault://pass/{ID}#code"),
        &format!("haex-vault://pass/{ID}/extra"),
    ] {
        assert_eq!(id_of_link(other), None, "{other}");
    }
}

#[test]
fn a_label_is_trimmed_cleaned_and_bounded() {
    assert_eq!(clean_label("  Concert  ").as_deref(), Some("Concert"));
    assert_eq!(clean_label("Con\ncert\u{0}").as_deref(), Some("Concert"));
    assert_eq!(clean_label(""), None);
    assert_eq!(clean_label("  \t "), None);
    assert_eq!(clean_label("\u{7}"), None);
    let long = "x".repeat(MAX_LABEL_CHARS + 20);
    assert_eq!(
        clean_label(&long).map(|l| l.chars().count()),
        Some(MAX_LABEL_CHARS)
    );
}

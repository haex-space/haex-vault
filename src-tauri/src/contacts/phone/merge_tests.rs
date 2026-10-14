use super::*;

fn book(entries: &[(&str, Keys)]) -> Vec<(String, Keys)> {
    entries
        .iter()
        .map(|(id, keys)| ((*id).to_owned(), keys.clone()))
        .collect()
}

fn anna_on_phone() -> Keys {
    keys(
        Some("Anna Beck"),
        Some("Anna"),
        Some("Beck"),
        &["+49 170 1234567"],
        &[],
    )
}

#[test]
fn same_name_and_number_in_other_forms_is_clear() {
    let holzi = keys(Some("Beck Anna"), None, None, &["0170 1234567"], &[]);
    assert_eq!(
        find(&anna_on_phone(), &book(&[("h1", holzi)])),
        Match::Clear("h1".to_owned())
    );
}

#[test]
fn the_name_alone_is_uncertain() {
    let holzi = keys(None, Some("anna"), Some("BECK"), &["030 5550101"], &[]);
    assert_eq!(
        find(&anna_on_phone(), &book(&[("h1", holzi)])),
        Match::Uncertain(vec!["h1".to_owned()])
    );
}

#[test]
fn the_email_alone_is_uncertain() {
    let phone = keys(Some("A. Beck"), None, None, &[], &["Anna@Example.org "]);
    let holzi = keys(Some("Anna Beck"), None, None, &[], &["anna@example.org"]);
    assert_eq!(
        find(&phone, &book(&[("h1", holzi)])),
        Match::Uncertain(vec!["h1".to_owned()])
    );
}

#[test]
fn two_candidates_are_uncertain() {
    let first = keys(Some("Anna Beck"), None, None, &["0170 1234567"], &[]);
    let second = keys(Some("Anna Beck"), None, None, &[], &[]);
    assert_eq!(
        find(&anna_on_phone(), &book(&[("h1", first), ("h2", second)])),
        Match::Uncertain(vec!["h1".to_owned(), "h2".to_owned()])
    );
}

#[test]
fn nothing_in_common_is_no_match() {
    let holzi = keys(Some("Bernd Clausen"), None, None, &["0171 7654321"], &[]);
    assert_eq!(find(&anna_on_phone(), &book(&[("h1", holzi)])), Match::None);
}

#[test]
fn numbers_match_by_their_end_with_seven_digits_at_least() {
    assert!(phones_match(
        &phone_digits("+49 170 1234567"),
        &phone_digits("0170 123-45 67")
    ));
    assert!(
        !phones_match(&phone_digits("123456"), &phone_digits("0123456")),
        "six digits are too few"
    );
    assert!(!phones_match(
        &phone_digits("0170 1234567"),
        &phone_digits("0171 1234567")
    ));
}

#[test]
fn names_are_compared_without_case_and_extra_space() {
    assert_eq!(name_key("  Anna   BECK "), "anna beck");
    assert_eq!(
        name_key("Ａｎｎａ"),
        "anna",
        "NFKC folds full-width letters"
    );
}

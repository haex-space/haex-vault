//! Tests for what counts as personal data in a form or a request (spec 065 FR-011, amended
//! 2026-11-08).

use super::personal::{body_carries_personal, field_is_personal, form_carries_personal, FormField};

fn field(name: &str, value: &str, secret: bool, hint: &str) -> FormField {
    (name.to_owned(), value.to_owned(), secret, hint.to_owned())
}

#[test]
fn what_names_an_address_a_contact_or_money_is_personal() {
    for name in [
        "email",
        "E-Mail",
        "userEmail",
        "billing_street",
        "shippingAddress",
        "address_line1",
        "Straße",
        "plz",
        "zip",
        "postal_code",
        "city",
        "phone",
        "tel",
        "mobile",
        "first_name",
        "lastName",
        "vorname",
        "date_of_birth",
        "iban",
        "card_number",
        "cardholder",
        "cc-number",
        "password",
        "confirmPassword",
        "passwd",
        "cookies",
        "sessionId",
    ] {
        assert!(field_is_personal(name, "x", false, ""), "{name}");
    }
}

#[test]
fn a_search_a_filter_or_a_comment_is_not() {
    for (name, value) in [
        ("q", "mobland staffel 1"),
        ("query", "wetter berlin"),
        ("search", "kaffee"),
        ("s", "hallo"),
        ("page", "2"),
        ("sort", "price"),
        ("category", "serien"),
        ("comment", "Schöner Artikel"),
        ("quantity", "3"),
        ("newsletter", "on"),
        ("keyword", "film"),
    ] {
        assert!(!field_is_personal(name, value, false, ""), "{name}");
    }
}

#[test]
fn the_kind_a_field_declares_counts_whatever_its_name() {
    for hint in [
        "email",
        "tel",
        "password",
        "text new-password",
        "text street-address",
        "text postal-code",
        "text cc-number",
        "text given-name",
        "text bday",
    ] {
        assert!(field_is_personal("x1", "wert", false, hint), "{hint}");
    }
    for hint in ["", "text", "search", "text off", "checkbox", "number"] {
        assert!(!field_is_personal("x1", "wert", false, hint), "{hint}");
    }
}

#[test]
fn a_secret_or_an_email_address_in_a_value_is_personal_and_an_empty_field_is_not() {
    assert!(field_is_personal("x1", "geheim", true, ""));
    assert!(field_is_personal("note", "anna@example.de", false, ""));
    assert!(!field_is_personal("note", "Treffen um drei", false, ""));
    assert!(!field_is_personal("email", "", false, "email"));
    assert!(!field_is_personal("email", "   ", false, "email"));
}

#[test]
fn a_form_with_a_password_field_is_a_credentials_form_even_when_the_field_is_empty() {
    assert!(field_is_personal("pass", "", false, "password"));
    assert!(field_is_personal("x", "", false, "text new-password"));
    assert!(field_is_personal("x", "", true, ""));
    assert!(form_carries_personal(&[
        field("user", "admin", false, "text"),
        field("pass", "", true, "password"),
    ]));
}

#[test]
fn a_form_is_personal_when_one_of_its_fields_is() {
    let search = vec![field("q", "mobland", false, "search")];
    assert!(!form_carries_personal(&search));
    let filter = vec![
        field("sort", "price", false, ""),
        field("category", "serien", false, "text"),
    ];
    assert!(!form_carries_personal(&filter));
    let registration = vec![
        field("username", "anna", false, "text"),
        field("email", "a@b.de", false, "email"),
        field("password", "geheim", true, "password"),
    ];
    assert!(form_carries_personal(&registration));
    let address = vec![
        field("q", "x", false, ""),
        field("street", "Hauptstr. 1", false, ""),
    ];
    assert!(form_carries_personal(&address));
    assert!(!form_carries_personal(&[]));
}

#[test]
fn a_body_is_personal_by_its_form_fields_json_members_or_text() {
    for body in [
        "email=a%40b.de&password=geheim",
        "q=x&street=Hauptstr.+1",
        r#"{"user":{"email":"a@b.de"}}"#,
        r#"{"variables":{"input":{"iban":"DE02"}}}"#,
        r#"[{"name":"email","value":"a@b.de"}]"#,
        r#"{"comment":"mail me at anna@example.de"}"#,
        "--b\r\nContent-Disposition: form-data; name=\"email\"\r\n\r\na@b.de\r\n--b--",
    ] {
        assert!(body_carries_personal(body), "{body}");
    }
    for body in [
        "q=mobland&page=2",
        r#"{"query":"mobland","page":2}"#,
        r#"{"events":[{"type":"click","x":12,"y":30}]}"#,
        r#"{"email":""}"#,
        r#"{"comment":"Hallo","count":3}"#,
        "ping",
        "",
    ] {
        assert!(!body_carries_personal(body), "{body}");
    }
}

#[test]
fn a_very_deep_body_does_not_hang_the_check() {
    let deep = format!("{}1{}", r#"{"a":"#.repeat(200), "}".repeat(200));
    assert!(!body_carries_personal(&deep));
}

//! Tests for what counts as submitting (spec 065, T044, research R5).

use super::submit::{changing, click_submits, fields_of, named_submitting, shown, Facts};

fn facts(tag: &str, kind: &str, form: bool) -> Facts {
    Facts {
        tag: tag.to_owned(),
        kind: kind.to_owned(),
        form,
    }
}

#[test]
fn step_one_marks_what_submits_a_form() {
    let submitting = [
        (facts("input", "submit", true), "Weiter"),
        (facts("input", "image", true), ""),
        (facts("button", "", true), "Weiter"),
        (facts("button", "submit", true), "Speichern"),
    ];
    for (facts, name) in submitting {
        assert!(click_submits(&facts, name), "{facts:?} {name}");
    }
}

#[test]
fn step_one_marks_names_that_submit_wherever_they_are() {
    for name in [
        "Jetzt kaufen",
        "Bestellung absenden",
        "Zahlungspflichtig bestellen",
        "Konto löschen",
        "Beitrag veröffentlichen",
        "Überweisen",
        "Buy now",
        "Place order",
        "Proceed to checkout",
        "Delete account",
        "Cancel subscription",
        "Confirm",
    ] {
        assert!(named_submitting(name), "{name}");
        assert!(click_submits(&facts("a", "", false), name), "{name}");
    }
}

#[test]
fn a_plain_link_or_a_disclosure_button_does_not_submit() {
    let plain = [
        (facts("a", "", false), "Zum Artikel"),
        (facts("button", "button", true), "Mehr anzeigen"),
        (facts("button", "", false), "Details"),
        (facts("summary", "", false), "Versandkosten"),
        (facts("input", "reset", true), "Zurücksetzen"),
        (facts("input", "checkbox", true), "Newsletter"),
        (facts("a", "", false), "Startseite"),
    ];
    for (facts, name) in plain {
        assert!(!click_submits(&facts, name), "{facts:?} {name}");
    }
}

#[test]
fn step_two_holds_requests_that_may_change_something() {
    for method in ["POST", "put", "Patch", "DELETE"] {
        assert!(changing(method), "{method}");
    }
    for method in ["GET", "HEAD", "OPTIONS"] {
        assert!(!changing(method), "{method}");
    }
}

#[test]
fn the_dialog_shows_form_and_json_fields_with_secrets_hidden() {
    let form = fields_of(Some(
        "email=a%40b.de&password=geheim&menge=2&card_number=4111",
    ));
    let pairs: Vec<(&str, &str)> = form
        .iter()
        .map(|field| (field.name.as_str(), field.value.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("email", "a@b.de"),
            ("password", "•••"),
            ("menge", "2"),
            ("card_number", "•••"),
        ]
    );

    let json = fields_of(Some(r#"{"comment":"Hallo","authToken":"x","count":3}"#));
    let pairs: Vec<(&str, &str)> = json
        .iter()
        .map(|field| (field.name.as_str(), field.value.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [("comment", "Hallo"), ("authToken", "•••"), ("count", "3")]
    );

    assert!(fields_of(None).is_empty());
    assert!(fields_of(Some("binary\u{0}data")).is_empty());
    assert_eq!(
        shown("kommentar", "x", true).value,
        "•••",
        "the page says so"
    );
}

//! Tests for what counts as submitting (spec 065, T044, research R5).

use super::personal::FormField;
use super::submit::{
    changing, click_submits, element_asks, fields_of, named_submitting, request_asks, shown, Facts,
};

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
        "Bestellung abschließen",
        "Zahlungspflichtig bestellen",
        "Konto löschen",
        "Beitrag veröffentlichen",
        "Überweisen",
        "Buy now",
        "Place order",
        "Proceed to checkout",
        "Delete account",
        "Cancel subscription",
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
        (facts("a", "", false), "Abmelden"),
        (facts("a", "", false), "Sign out"),
        (
            facts("a", "", false),
            "Newsletter abbestellen (unsubscribe)",
        ),
        (facts("a", "", false), "Confirm"),
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

#[test]
fn nested_json_and_other_text_are_shown_not_hidden() {
    let pairs = |body: &str| -> Vec<(String, String)> {
        fields_of(Some(body))
            .into_iter()
            .map(|field| (field.name, field.value))
            .collect()
    };

    // A list in a member (a tracker's batch) is shown, only what is named like a secret is not.
    assert_eq!(
        pairs(r#"{"xhr":[{"params":{"page":"/wetter","token":"geheim"},"start":12}]}"#),
        [(
            "xhr".to_owned(),
            r#"[{"params":{"page":"/wetter","token":"•••"},"start":12}]"#.to_owned()
        )]
    );
    assert_eq!(
        pairs(r#"{"user":{"password":"x","name":"anna"}}"#),
        [(
            "user".to_owned(),
            r#"{"password":"•••","name":"anna"}"#.to_owned()
        )]
    );

    // A name/value pair (a form sent as JSON) names its secret in a value: its value is hidden.
    assert_eq!(
        pairs(r#"[{"name":"email","value":"a@b.de"},{"name":"password","value":"geheim"}]"#),
        [(
            String::new(),
            r#"[{"name":"email","value":"a@b.de"},{"name":"password","value":"•••"}]"#.to_owned()
        )]
    );
    assert_eq!(
        pairs(r#"{"field":"card_number","value":"4111","page":"/kasse"}"#),
        [
            ("field".to_owned(), "card_number".to_owned()),
            ("value".to_owned(), "•••".to_owned()),
            ("page".to_owned(), "•••".to_owned()),
        ]
    );

    // A JSON list and plain text are one field without a name.
    assert_eq!(
        pairs(r#"[{"a":1},{"a":2}]"#),
        [(String::new(), r#"[{"a":1},{"a":2}]"#.to_owned())]
    );
    assert_eq!(
        pairs("{\"a\":1}\n{\"a\":2}"),
        [(String::new(), "{\"a\":1}\n{\"a\":2}".to_owned())]
    );
    assert_eq!(pairs("ping"), [(String::new(), "ping".to_owned())]);

    // Text that mentions a secret is not shown at all.
    assert_eq!(
        pairs("--b\r\nContent-Disposition: form-data; name=\"password\"\r\n\r\ngeheim"),
        [(String::new(), "•••".to_owned())]
    );

    // A long value is cut, binary data is left out.
    let long = format!(r#"{{"list":[{}]}}"#, "1,".repeat(300) + "1");
    assert!(pairs(&long)[0].1.ends_with('…'));
    assert!(fields_of(Some("a\u{1}b")).is_empty());
}

fn field(name: &str, value: &str, secret: bool, hint: &str) -> FormField {
    (name.to_owned(), value.to_owned(), secret, hint.to_owned())
}

#[test]
fn a_click_asks_for_what_costs_or_deletes_and_for_a_form_with_personal_data() {
    let button = facts("button", "submit", true);
    let search = [field("q", "mobland", false, "search")];
    let registration = [
        field("email", "a@b.de", false, "email"),
        field("password", "geheim", true, "password"),
    ];

    // A form of no personal data runs: a search, a filter, a comment.
    assert!(!element_asks("button", &button, "Suchen", &search));
    assert!(!element_asks("button", &button, "Senden", &search));
    // Personal data asks.
    assert!(element_asks("button", &button, "Weiter", &registration));
    assert!(element_asks(
        "button",
        &button,
        "Konto anlegen",
        &registration
    ));
    // What costs or deletes asks wherever it is and whatever the form holds.
    for name in [
        "Jetzt kaufen",
        "Konto löschen",
        "Beitrag veröffentlichen",
        "Pay now",
    ] {
        assert!(
            element_asks("button", &facts("a", "", false), name, &[]),
            "{name}"
        );
    }
    // Signing out asks nothing.
    assert!(!element_asks(
        "link",
        &facts("a", "", false),
        "Abmelden",
        &[]
    ));
    // A link that is no form never asks for its fields.
    assert!(!element_asks(
        "link",
        &facts("a", "", false),
        "Weiter",
        &registration
    ));
    // Typing and ticking ask nothing here.
    for kind in ["field", "select", "checkbox", "radio"] {
        assert!(
            !element_asks(kind, &button, "Kaufen", &registration),
            "{kind}"
        );
    }
}

#[test]
fn a_held_request_asks_only_for_deleting_personal_data_or_what_cannot_be_told() {
    let url = "https://shop.example/api";
    // Reading goes.
    assert!(!request_asks("GET", url, None));
    assert!(!request_asks("HEAD", url, None));
    // A request that changes something but sends nothing personal goes.
    assert!(!request_asks("POST", url, None));
    assert!(!request_asks("POST", url, Some("q=mobland&page=2")));
    assert!(!request_asks(
        "POST",
        url,
        Some(r#"{"events":[{"type":"click"}]}"#)
    ));
    assert!(!request_asks("PUT", url, Some(r#"{"volume":5}"#)));
    // Personal data asks.
    assert!(request_asks("POST", url, Some("email=a%40b.de&password=x")));
    assert!(request_asks(
        "PATCH",
        url,
        Some(r#"{"address":{"street":"Hauptstr. 1"}}"#)
    ));
    // Deleting asks, with or without a body.
    assert!(request_asks("DELETE", url, None));
    assert!(request_asks("delete", url, Some("{}")));
    // An address that names a purchase, a registration or a deletion asks.
    for path in [
        "checkout", "payment", "register", "signup", "kasse", "delete",
    ] {
        assert!(
            request_asks("POST", &format!("https://shop.example/{path}"), Some("a=1")),
            "{path}"
        );
    }
    assert!(request_asks(
        "POST",
        "https://shop.example/de/Bestellen/x",
        None
    ));
    assert!(!request_asks(
        "POST",
        "https://shop.example/search",
        Some("q=x")
    ));
    // A body that is no text cannot be told.
    assert!(request_asks("POST", url, Some("a\u{1}b")));
}

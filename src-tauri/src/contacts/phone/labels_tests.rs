use super::*;

#[test]
fn every_phone_type_comes_back_as_the_same_type() {
    for kind in [PHONE, EMAIL, ADDRESS, SIP, IM, WEBSITE, EVENT, RELATION] {
        for (code, label) in table(kind) {
            let read = from_phone(kind, Some(&code.to_string()), None);
            assert_eq!(read, *label, "{kind} {code}");
            // Every type comes back as itself, the ones without a holzi label included.
            assert_eq!(to_phone(kind, &read), (*code, None), "{kind} {label}");
        }
    }
}

#[test]
fn a_custom_type_keeps_its_own_label() {
    assert_eq!(
        from_phone(PHONE, Some("0"), Some("Ferienhaus")),
        "custom:Ferienhaus"
    );
    assert_eq!(
        to_phone(PHONE, "custom:Ferienhaus"),
        (CUSTOM_TYPE, Some("Ferienhaus".to_owned()))
    );
    assert_eq!(from_phone(PHONE, Some("0"), None), "other");
}

#[test]
fn a_missing_or_unknown_type_reads_as_other() {
    assert_eq!(from_phone(EMAIL, None, None), "other");
    assert_eq!(from_phone(EMAIL, Some("99"), None), "other");
    assert_eq!(from_phone(EMAIL, Some("not a number"), None), "other");
}

#[test]
fn messenger_services_come_from_the_protocol_or_its_custom_name() {
    assert_eq!(service(Some("3"), None).as_deref(), Some("Skype"));
    assert_eq!(
        service(Some("-1"), Some("Signal")).as_deref(),
        Some("Signal")
    );
    assert_eq!(service(None, Some("Threema")).as_deref(), Some("Threema"));
    assert_eq!(service(Some("-1"), None), None);
}

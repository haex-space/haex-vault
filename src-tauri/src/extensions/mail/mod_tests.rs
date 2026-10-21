use super::*;

#[test]
fn only_this_device_may_be_reached_without_encryption() {
    for host in ["localhost", "127.0.0.1", "::1", "[::1]", "LOCALHOST"] {
        assert!(
            check_server(host, 1143, ConnectionSecurity::None).is_ok(),
            "{host}"
        );
    }
    for host in ["imap.example.org", "192.168.1.2", "localhost.example.org"] {
        assert!(
            check_server(host, 143, ConnectionSecurity::None).is_err(),
            "{host}"
        );
        assert!(
            check_server(host, 143, ConnectionSecurity::StartTls).is_ok(),
            "{host}"
        );
    }
    assert!(check_server("", 993, ConnectionSecurity::Tls).is_err());
    assert!(check_server("a b", 993, ConnectionSecurity::Tls).is_err());
    assert!(check_server("imap.example.org", 0, ConnectionSecurity::Tls).is_err());
}

#[test]
fn a_flag_is_a_system_flag_or_an_atom_and_never_ends_the_command() {
    for ok in ["\\Seen", "\\Flagged", "$Forwarded", "Junk", "NonJunk"] {
        assert!(check_flag(ok).is_ok(), "{ok}");
    }
    for bad in [
        "",
        "\\",
        "Seen)",
        "(Seen",
        "a b",
        "x\r\nA1 DELETE INBOX",
        "\"quoted\"",
        "{5}",
        "\\Seen\\Deleted",
        "ä",
    ] {
        assert!(check_flag(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_mailbox_name_has_no_control_characters() {
    assert!(check_mailbox("INBOX").is_ok());
    assert!(check_mailbox("Gesendete Objekte/2026").is_ok());
    assert!(check_mailbox("").is_err());
    assert!(check_mailbox("INBOX\r\nA1 LOGOUT").is_err());
}

#[test]
fn errors_name_no_secret_and_map_to_bridge_codes() {
    let auth: BridgeError = MailError::Auth.into();
    assert_eq!(auth.code.as_u16(), 2005);
    assert!(!auth.message.contains("password"));
    let large: BridgeError = MailError::TooLarge.into();
    assert_eq!(large.code.as_u16(), 7000);
    let invalid: BridgeError = MailError::Invalid("x".into()).into();
    assert_eq!(invalid.code.as_u16(), 3001);
}

#[test]
fn a_date_is_a_real_day_written_year_month_day() {
    assert_eq!(parse_date("2026-01-02"), Some((2026, 1, 2)));
    assert_eq!(parse_date("2024-02-29"), Some((2024, 2, 29)), "a leap year");
    for bad in [
        "2026-02-29",
        "2026-13-01",
        "2026-00-10",
        "2026-04-31",
        "26-01-02",
        "2026-1-2",
        "2026-01-02-03",
        "yesterday",
        "",
    ] {
        assert_eq!(parse_date(bad), None, "{bad:?}");
    }
}

#[test]
fn a_search_needs_a_criterion_and_texts_that_cannot_end_the_command() {
    let text = |t: &str| SearchCriteria {
        text: Some(t.into()),
        ..SearchCriteria::default()
    };
    assert!(check_criteria(&SearchCriteria::default()).is_err());
    assert!(check_criteria(&text("Rechnung")).is_ok());
    assert!(check_criteria(&text(&"a".repeat(MAX_SEARCH_TEXT))).is_ok());
    assert!(check_criteria(&text(&"a".repeat(MAX_SEARCH_TEXT + 1))).is_err());
    for bad in ["", "  ", "a\r\nA1 DELETE INBOX", "a\0b", "tab\there"] {
        assert!(check_criteria(&text(bad)).is_err(), "{bad:?}");
    }
    let flags = SearchCriteria {
        unseen: Some(false),
        ..SearchCriteria::default()
    };
    assert!(
        check_criteria(&flags).is_ok(),
        "a flag alone is a criterion"
    );
    let date = SearchCriteria {
        before: Some("2026-02-30".into()),
        ..SearchCriteria::default()
    };
    assert!(check_criteria(&date).is_err());
}

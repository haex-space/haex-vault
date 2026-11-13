//! The reader of pass files (spec 092, T063): each style gives its kind, the fields of the file
//! become the pass, texts follow the language of the window, and a file that is not safe to read is
//! refused with a reason. Files are built in the tests with throwaway data; no real pass is used.

use serde_json::json;

use super::model::{PassKind, Start};
use super::pkpass::{read, Refusal, WHY_UPDATE_SERVICE};
use super::pkpass_fixtures::{file, train, zip, PNG};

#[test]
fn a_boarding_pass_becomes_a_pass_with_everything_the_file_gives() {
    let bytes = file(&train(), &[("logo.png", PNG)]);
    let pass = read(&bytes, "de").expect("read");
    assert_eq!(pass.pass_kind, PassKind::BoardingPass);
    assert!(pass.ticket_like);
    assert_eq!(pass.provider.as_deref(), Some("Example Rail"));
    assert_eq!(
        pass.start,
        Some(Start::At {
            at: "2026-11-14T07:30:00.000Z".to_owned(),
            zone: "+01:00".to_owned()
        })
    );
    assert_eq!(pass.expires_at.as_deref(), Some("2026-11-14"));
    assert_eq!(pass.source_id.as_deref(), Some("pass.example.rail/S-1"));
    assert_eq!(pass.icon.as_deref(), Some(PNG));
    // The title says what, where and when, and holds the provider's name.
    assert_eq!(pass.title, "Example Rail: ICE Berlin → München, 14.11.");
    let formats: Vec<_> = pass.codes.iter().map(|c| c.format.as_str()).collect();
    assert_eq!(formats, ["aztec", "qr"]);
    assert_eq!(pass.codes[0].label.as_deref(), Some("Ticket"));
    assert!(pass.codes.iter().all(|c| c.flag.is_none()));
    let names: Vec<_> = pass.key_values.iter().map(|kv| kv.key.as_str()).collect();
    assert_eq!(names, ["From", "To", "Seat", "Terms"]);
}

#[test]
fn the_title_of_another_language_writes_the_day_its_way() {
    let bytes = file(&train(), &[]);
    let pass = read(&bytes, "en").expect("read");
    assert_eq!(pass.title, "Example Rail: ICE Berlin → München, 2026-11-14");
}

#[test]
fn every_style_gives_its_kind() {
    for (style, kind, ticket) in [
        ("boardingPass", PassKind::BoardingPass, true),
        ("eventTicket", PassKind::Ticket, true),
        ("coupon", PassKind::Voucher, false),
        ("storeCard", PassKind::LoyaltyCard, false),
        ("generic", PassKind::Other, false),
    ] {
        let mut pass =
            json!({ "formatVersion": 1, "organizationName": "Shop", "description": "Card" });
        pass[style] = json!({ "primaryFields": [] });
        let read = read(&file(&pass, &[]), "en").expect("read");
        assert_eq!(read.pass_kind, kind, "{style}");
        assert_eq!(read.ticket_like, ticket, "{style}");
    }
}

#[test]
fn a_store_card_without_a_date_has_none_and_is_named_by_its_provider() {
    let pass = json!({
        "formatVersion": 1,
        "organizationName": "Corner Shop",
        "description": "Customer card",
        "storeCard": { "primaryFields": [{ "key": "points", "label": "Points", "value": 120 }] },
        "barcode": { "format": "PKBarcodeFormatCode128", "message": "123-ABC" }
    });
    let read = read(&file(&pass, &[]), "en").expect("read");
    assert_eq!(read.start, None);
    assert_eq!(read.title, "Corner Shop: Customer card");
    // The legacy single barcode counts as a code.
    assert_eq!(read.codes.len(), 1);
    assert_eq!(read.codes[0].format, "code128");
    assert_eq!(read.key_values[0].value.as_deref(), Some("120"));
}

#[test]
fn texts_follow_the_language_of_the_window_and_fall_back_to_the_base_text() {
    let pass = json!({
        "formatVersion": 1,
        "organizationName": "Cinema",
        "description": "TICKET_NAME",
        "eventTicket": {
            "primaryFields": [{ "key": "event", "label": "EVENT_LABEL", "value": "Concert" }]
        }
    });
    // UTF-16 with a mark, as Apple writes them.
    let mut german: Vec<u8> = vec![0xFF, 0xFE];
    for unit in
        "/* de */\n\"TICKET_NAME\" = \"Eintrittskarte\";\n\"EVENT_LABEL\" = \"Veranstaltung\";\n"
            .encode_utf16()
    {
        german.extend_from_slice(&unit.to_le_bytes());
    }
    let bytes = file(&pass, &[("de.lproj/pass.strings", &german)]);
    let de = read(&bytes, "de").expect("de");
    assert_eq!(de.key_values[0].key, "Veranstaltung");
    let en = read(&bytes, "en").expect("en");
    assert_eq!(en.key_values[0].key, "EVENT_LABEL");
    // A region of the language is used too.
    let regional = file(&pass, &[("de-DE.lproj/pass.strings", &german)]);
    assert_eq!(
        read(&regional, "de").expect("de").key_values[0].key,
        "Veranstaltung"
    );
}

#[test]
fn a_code_that_does_not_fit_is_kept_as_text_with_a_flag() {
    let pass = json!({
        "formatVersion": 1,
        "organizationName": "Shop",
        "generic": {},
        "barcodes": [
            { "format": "PKBarcodeFormatCode39", "message": "ABC" },
            { "format": "PKBarcodeFormatQR", "message": "ok" },
            { "message": "no format" }
        ]
    });
    let pass = read(&file(&pass, &[]), "en").expect("read");
    assert_eq!(pass.codes[0].flag.as_deref(), Some("unknown_format"));
    assert_eq!(pass.codes[0].format, "code39");
    assert_eq!(pass.codes[1].flag, None);
    assert_eq!(pass.codes[2].flag.as_deref(), Some("unknown_format"));
}

#[test]
fn the_update_service_is_named_and_never_called() {
    let pass = read(&file(&train(), &[]), "en").expect("read");
    let reasons = |what: &str| {
        pass.not_kept
            .iter()
            .find(|n| n.what == what)
            .map(|n| n.why.clone())
    };
    assert_eq!(
        reasons("webServiceURL").as_deref(),
        Some(WHY_UPDATE_SERVICE)
    );
    assert_eq!(
        reasons("authenticationToken").as_deref(),
        Some(WHY_UPDATE_SERVICE)
    );
    assert!(reasons("backgroundColor").is_some());
    assert!(reasons("boardingPass.transitType").is_none());
    assert!(reasons("transitType").is_some());
    assert!(reasons("fields.changeMessage").is_some());
    // The report names a thing, never a value.
    let text = format!("{:?}", pass.not_kept);
    assert!(!text.contains("updates.example.org"), "{text}");
    assert!(!text.contains("token-token"), "{text}");
}

#[test]
fn two_fields_with_one_name_do_not_overwrite_each_other() {
    let pass = json!({
        "formatVersion": 1,
        "organizationName": "Shop",
        "generic": {
            "primaryFields": [{ "key": "a", "label": "Note", "value": "one" }],
            "backFields": [{ "key": "b", "label": "note", "value": "two" }]
        }
    });
    let pass = read(&file(&pass, &[]), "en").expect("read");
    let names: Vec<_> = pass.key_values.iter().map(|kv| kv.key.as_str()).collect();
    assert_eq!(names, ["Note", "note (2)"]);
}

#[test]
fn pictures_that_are_not_the_logo_are_named_not_kept() {
    let bytes = file(
        &train(),
        &[("strip.png", PNG), ("logo@2x.png", PNG), ("logo.png", PNG)],
    );
    let pass = read(&bytes, "en").expect("read");
    assert!(pass.not_kept.iter().any(|n| n.what == "strip.png"));
    assert!(pass.icon.is_some());
    // A logo that is no PNG is not an icon and is named too.
    let bytes = file(&train(), &[("logo.png", b"not a picture")]);
    let pass = read(&bytes, "en").expect("read");
    assert!(pass.icon.is_none());
    assert!(pass.not_kept.iter().any(|n| n.what == "logo.png"));
}

fn refused(bytes: &[u8]) -> Refusal {
    read(bytes, "en").expect_err("refused")
}

#[test]
fn a_file_that_is_no_pass_is_refused_with_its_reason() {
    assert_eq!(refused(b"this is not a zip"), Refusal::NotAZip);
    assert_eq!(refused(&zip(&[("readme.txt", b"hi")])), Refusal::NoPassJson);
    assert_eq!(
        refused(&zip(&[("pass.json", b"{ not json")])),
        Refusal::BadPassJson
    );
    assert_eq!(
        refused(&zip(&[("pass.json", b"[1, 2]")])),
        Refusal::BadPassJson
    );
    let no_style = json!({ "formatVersion": 1, "organizationName": "X" });
    assert_eq!(refused(&file(&no_style, &[])), Refusal::NoStyle);
    let future = json!({ "formatVersion": 2, "generic": {} });
    assert_eq!(refused(&file(&future, &[])), Refusal::UnsupportedVersion);
}

#[test]
fn a_name_that_leaves_the_archive_is_refused() {
    for name in [
        "../evil",
        "a/../../evil",
        "/etc/passwd",
        "C:/windows/x",
        "a\\b",
    ] {
        let bytes = file(&train(), &[(name, b"x")]);
        assert_eq!(refused(&bytes), Refusal::UnsafeName, "{name}");
    }
}

#[test]
fn too_many_entries_and_too_much_data_are_refused() {
    let names: Vec<String> = (0..300).map(|i| format!("f{i}.txt")).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
    assert_eq!(refused(&zip(&entries)), Refusal::TooManyEntries);

    // One entry past its limit, built from noise so it does not shrink.
    let noise: Vec<u8> = (0..5 * 1024 * 1024u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    assert_eq!(
        refused(&file(&train(), &[("big.bin", &noise)])),
        Refusal::TooLarge
    );
}

#[test]
fn an_entry_that_inflates_far_beyond_what_it_is_stored_as_is_refused() {
    let zeros = vec![0u8; 3 * 1024 * 1024];
    assert_eq!(
        refused(&file(&train(), &[("bomb.bin", &zeros)])),
        Refusal::Inflated
    );
}

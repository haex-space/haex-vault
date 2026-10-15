use serde_json::json;

use super::*;

fn data_row(data_id: i64, mimetype: &str, fields: Json) -> DataRow {
    DataRow {
        data_id,
        mimetype: mimetype.to_owned(),
        is_read_only: false,
        fields: fields.as_object().expect("an object").clone(),
    }
}

/// Every kind that goes to a phone, written as a new row and read back, is the same value.
#[test]
fn every_phone_kind_survives_the_way_there_and_back() {
    let values = [
        (PHONE, "mobile", "+49 170 1234567"),
        (PHONE, "fax_work", "030 5550199"),
        (PHONE, "custom:Ferienhaus", "038 1234"),
        (EMAIL, "work", "anna@example.org"),
        (WEBSITE, "home", "https://anna.example.org"),
        (SIP, "work", "anna@sip.example.org"),
        (
            ADDRESS,
            "home",
            r#"{"city":"Leipzig","country":"DE","postcode":"04109","street":"Markt 1"}"#,
        ),
        (IM, "other", r#"{"handle":"@anna","service":"Signal"}"#),
        (EVENT, "birthday", r#"{"date":"1990-05-17"}"#),
        (EVENT, "anniversary", r#"{"date":"--06-01"}"#),
        (RELATION, "sister", "Berta Beck"),
    ];
    for (n, (kind, label, value)) in values.into_iter().enumerate() {
        let (mimetype, fields) = insert_fields(kind, label, value).expect("goes to a phone");
        let row = data_row(n as i64, mimetype, Json::Object(fields));
        let side = map_raw(&[row]);
        assert_eq!(side.values.len(), 1, "{kind} {value}");
        let read = &side.values[0];
        assert_eq!(read.kind, kind);
        assert_eq!(read.label, label, "{kind}");
        assert!(
            same_value(kind, &read.value, value),
            "{kind}: {} vs {value}",
            read.value
        );
    }
}

#[test]
fn holzi_only_kinds_never_go_to_a_phone() {
    for kind in [
        "custom",
        "holzi_vault",
        "holzi_nostr_relay",
        "holzi_iroh",
        "telegram_future_kind",
    ] {
        assert!(insert_fields(kind, "other", "x").is_none(), "{kind}");
    }
}

#[test]
fn head_rows_map_to_head_fields_and_back() {
    let rows = [
        data_row(
            1,
            "StructuredName",
            json!({ "display": "Dr. Anna Beck", "given": "Anna", "family": "Beck", "prefix": "Dr." }),
        ),
        data_row(2, "Nickname", json!({ "name": "Anni" })),
        data_row(
            3,
            "Organization",
            json!({ "company": "Beck GmbH", "title": "Chefin" }),
        ),
        data_row(4, "Note", json!({ "note": "Kennt Bernd" })),
    ];
    let side = map_raw(&rows);
    assert_eq!(
        side.head.get("given_name").map(String::as_str),
        Some("Anna")
    );
    assert_eq!(
        side.head.get("name_prefix").map(String::as_str),
        Some("Dr.")
    );
    assert_eq!(side.head.get("nickname").map(String::as_str), Some("Anni"));
    assert_eq!(
        side.head.get("job_title").map(String::as_str),
        Some("Chefin")
    );
    assert_eq!(
        side.head.get("note").map(String::as_str),
        Some("Kennt Bernd")
    );
    assert_eq!(side.head_rows.get("Organization"), Some(&3));

    let columns = BTreeMap::from([("organization", Some("Beck AG")), ("job_title", None)]);
    let fields = head_fields("Organization", &columns);
    assert_eq!(
        Json::Object(fields),
        json!({ "company": "Beck AG", "title": null })
    );
}

#[test]
fn a_messenger_of_a_known_protocol_or_a_custom_one_is_read() {
    let rows = [
        data_row(
            1,
            "Im",
            json!({ "data": "anna.beck", "protocol": "3", "type": "1" }),
        ),
        data_row(
            2,
            "Im",
            json!({ "data": "@anna", "protocol": "-1", "customProtocol": "Signal" }),
        ),
    ];
    let side = map_raw(&rows);
    let services: Vec<&str> = side
        .values
        .iter()
        .map(|value| value.value.as_str())
        .collect();
    assert_eq!(
        services,
        vec![
            r#"{"handle":"anna.beck","service":"Skype"}"#,
            r#"{"handle":"@anna","service":"Signal"}"#
        ]
    );
    // holzi writes every service with PROTOCOL_CUSTOM.
    let (_, fields) = insert_fields(IM, "home", services[0]).expect("a messenger");
    assert_eq!(fields.get("protocol"), Some(&json!("-1")));
    assert_eq!(fields.get("customProtocol"), Some(&json!("Skype")));
}

#[test]
fn rows_holzi_cannot_store_are_not_mapped() {
    let rows = [
        data_row(1, "Event", json!({ "startDate": "17. Mai", "type": "3" })),
        data_row(2, "Phone", json!({ "type": "2" })),
        data_row(3, "Email", json!({ "value": "no address" })),
        data_row(4, "Photo", json!({ "photo": "…" })),
        data_row(5, "vnd.com.whatsapp.profile", json!({ "data1": "x" })),
    ];
    assert_eq!(map_raw(&rows), PhoneSide::default());
}

#[test]
fn a_date_with_a_time_is_cut_to_the_day() {
    let side = map_raw(&[data_row(
        1,
        "Event",
        json!({ "startDate": "1990-05-17T00:00:00Z", "type": "3" }),
    )]);
    assert_eq!(side.values[0].value, r#"{"date":"1990-05-17"}"#);
}

#[test]
fn an_address_only_as_one_text_goes_into_the_street() {
    let side = map_raw(&[data_row(
        1,
        "StructuredPostal",
        json!({ "formatted": "Markt 1, Leipzig" }),
    )]);
    assert_eq!(side.values[0].value, r#"{"street":"Markt 1, Leipzig"}"#);
}

#[test]
fn an_update_names_only_what_changed() {
    let old = PhoneValue {
        data_id: 1,
        kind: PHONE.to_owned(),
        label: "mobile".to_owned(),
        value: "0170 1".to_owned(),
    };
    assert_eq!(
        Json::Object(update_fields(&old, "mobile", "0170 2")),
        json!({ "value": "0170 2" })
    );
    assert_eq!(
        Json::Object(update_fields(&old, "work", "0170 1")),
        json!({ "type": "3", "label": null })
    );
    assert!(update_fields(&old, "mobile", " 0170 1 ").is_empty());
}

#[test]
fn values_compare_by_their_meaning() {
    assert!(same_value(PHONE, "0170 123-4567", "01701234567"));
    assert!(same_value(EMAIL, "Anna@Example.org", "anna@example.org"));
    assert!(same_value(
        ADDRESS,
        r#"{"street":"Markt 1","city":"Leipzig","region":""}"#,
        r#"{"city":"Leipzig","street":"Markt 1"}"#
    ));
    assert!(same_value(PHONE, "+49 170 1234567", "0170 1234567"));
    assert!(!same_value(PHONE, "0170 1234567", "0171 1234567"));
}

#[test]
fn a_membership_is_its_groups_title_and_system_groups_are_not_mapped() {
    let side = map_raw(&[
        data_row(
            1,
            "GroupMembership",
            json!({ "groupId": "5", "title": "Familie", "readOnly": false }),
        ),
        data_row(
            2,
            "GroupMembership",
            json!({ "groupId": "6", "title": "My Contacts", "readOnly": true }),
        ),
        data_row(3, "GroupMembership", json!({ "groupId": "7" })),
    ]);
    let groups: Vec<(&str, &str)> = side
        .values
        .iter()
        .map(|value| (value.kind.as_str(), value.value.as_str()))
        .collect();
    assert_eq!(groups, [(GROUP, "Familie")]);

    let (mimetype, fields) = insert_fields(GROUP, "", "Arbeit").expect("a membership");
    assert_eq!(mimetype, "GroupMembership");
    assert_eq!(
        Json::Object(fields),
        json!({ "title": "Arbeit" }),
        "no type, no label"
    );
    assert_eq!(group_value_id(" Arbeit "), "group:Arbeit");
}

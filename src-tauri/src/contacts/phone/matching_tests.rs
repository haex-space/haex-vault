use serde_json::json;
use tauri_plugin_holzi_android::contacts::{DataRow, RawIndex};

use super::*;

fn raw(id: i64, lookup: &str) -> RawIndex {
    RawIndex {
        id,
        version: 1,
        deleted: false,
        account_type: String::new(),
        account_name: String::new(),
        source_id: None,
        contact_id: id,
        lookup_key: lookup.to_owned(),
        last_updated: None,
        read_only: false,
    }
}

fn data(id: i64, given: &str, family: &str, phone: &str) -> RawData {
    RawData {
        id,
        has_photo: false,
        data: vec![
            DataRow {
                data_id: id * 10,
                mimetype: "StructuredName".to_owned(),
                is_read_only: false,
                fields: json!({ "given": given, "family": family })
                    .as_object()
                    .unwrap()
                    .clone(),
            },
            DataRow {
                data_id: id * 10 + 1,
                mimetype: "Phone".to_owned(),
                is_read_only: false,
                fields: json!({ "value": phone, "type": "2" })
                    .as_object()
                    .unwrap()
                    .clone(),
            },
        ],
    }
}

fn header(id: &str, name: &str, phone: &str) -> ContactHeader {
    ContactHeader {
        id: id.to_owned(),
        name: name.to_owned(),
        given_name: None,
        family_name: None,
        organization: None,
        phones: vec![phone.to_owned()],
        emails: vec![],
        groups: vec![],
        photo: None,
        origin_kind: crate::contacts::model::OriginKind::Holzi,
    }
}

fn group(key: &str, raws: Vec<RawIndex>) -> NewGroup {
    NewGroup {
        key: key.to_owned(),
        raws,
        joins: None,
    }
}

#[test]
fn a_clear_match_merges_and_an_uncertain_one_waits_for_the_person() {
    let groups = vec![
        group("g1", vec![raw(1, "g1")]),
        group("g2", vec![raw(2, "g2")]),
    ];
    let data = BTreeMap::from([
        (1, data(1, "Anna", "Beck", "+49 170 1234567")),
        (2, data(2, "Bernd", "Clausen", "0171 0000000")),
    ]);
    let book = vec![
        header("h-anna", "Beck Anna", "0170 1234567"),
        header("h-bernd", "Bernd Clausen", "040 1"),
    ];
    let review = match_groups(&groups, &data, &book, Some(&[]));
    assert_eq!(
        review.merges,
        BTreeMap::from([("g1".to_owned(), "h-anna".to_owned())])
    );
    assert_eq!(review.pairs.len(), 1);
    assert_eq!(review.pairs[0].contact_id, "h-bernd");
    assert_eq!(review.pairs[0].phone.name, "Bernd Clausen");

    let decided = [PairDecision {
        pair_id: review.pairs[0].id.clone(),
        merge: true,
    }];
    let confirmed = match_groups(&groups, &data, &book, Some(&decided));
    assert_eq!(
        confirmed.merges.get("g2").map(String::as_str),
        Some("h-bernd")
    );
}

#[test]
fn a_contact_two_groups_claim_clearly_is_uncertain_for_both() {
    let groups = vec![
        group("g1", vec![raw(1, "g1")]),
        group("g2", vec![raw(2, "g2")]),
    ];
    let data = BTreeMap::from([
        (1, data(1, "Anna", "Beck", "0170 1234567")),
        (2, data(2, "Anna", "Beck", "0170 1234567")),
    ]);
    let book = vec![header("h-anna", "Anna Beck", "0170 1234567")];
    let matched = match_groups(&groups, &data, &book, Some(&[]));
    assert!(matched.merges.is_empty());
    assert_eq!(matched.pairs.len(), 2);
}

#[test]
fn without_decisions_only_clear_matches_merge() {
    let groups = vec![group("g1", vec![raw(1, "g1")])];
    let data = BTreeMap::from([(1, data(1, "Anna", "Beck", "0170 9999999"))]);
    let book = vec![header("h-anna", "Anna Beck", "0170 1234567")];
    let matched = match_groups(&groups, &data, &book, None);
    assert!(matched.merges.is_empty());
    assert!(matched.pairs.is_empty());
}

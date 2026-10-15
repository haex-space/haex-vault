use serde::Deserialize;

use super::*;
use crate::contacts::model::OriginKind;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseContact {
    id: String,
    name: String,
    given_name: Option<String>,
    family_name: Option<String>,
    organization: Option<String>,
    phones: Vec<String>,
    emails: Vec<String>,
}

#[derive(Deserialize)]
struct Case {
    query: String,
    matches: Vec<String>,
}

#[derive(Deserialize)]
struct Cases {
    contacts: Vec<CaseContact>,
    cases: Vec<Case>,
}

fn header(contact: &CaseContact) -> ContactHeader {
    ContactHeader {
        id: contact.id.clone(),
        name: contact.name.clone(),
        given_name: contact.given_name.clone(),
        family_name: contact.family_name.clone(),
        organization: contact.organization.clone(),
        phones: contact.phones.clone(),
        emails: contact.emails.clone(),
        groups: vec![],
        photo: None,
        origin_kind: OriginKind::Holzi,
        targets: vec![],
        issue: None,
        has_card: false,
    }
}

#[test]
fn the_shared_cases_match_as_the_window_expects() {
    let cases: Cases =
        serde_json::from_str(include_str!("../../../src/lib/contacts/search-cases.json"))
            .expect("the shared cases parse");
    let haystacks: Vec<(String, String)> = cases
        .contacts
        .iter()
        .map(|contact| (contact.id.clone(), haystack(&header(contact))))
        .collect();
    for case in cases.cases {
        let found: Vec<String> = haystacks
            .iter()
            .filter(|(_, haystack)| matches(haystack, &case.query))
            .map(|(id, _)| id.clone())
            .collect();
        assert_eq!(found, case.matches, "query {:?}", case.query);
    }
}

#[test]
fn letters_in_a_query_never_turn_it_into_a_digit_search() {
    let mut contact = header(&CaseContact {
        id: "x".into(),
        name: "X".into(),
        given_name: None,
        family_name: None,
        organization: None,
        phones: vec!["0170 12".into()],
        emails: vec![],
    });
    contact.name = "Xaver".into();
    let haystack = haystack(&contact);
    assert!(matches(&haystack, "017012"));
    assert!(!matches(&haystack, "a017012"));
}

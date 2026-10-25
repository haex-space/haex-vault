use super::*;

fn head(display: Option<&str>, given: Option<&str>, family: Option<&str>) -> Head {
    Head {
        display_name: display.map(str::to_owned),
        given_name: given.map(str::to_owned),
        family_name: family.map(str::to_owned),
        ..Head::default()
    }
}

#[test]
fn first_and_last_name_win_over_a_stored_display_name() {
    let h = head(Some("npub1abc"), Some("Ada"), Some("Lovelace"));
    assert_eq!(derived_name(&h, &[], &[]), "Ada Lovelace");
}

#[test]
fn display_name_shows_while_there_are_no_name_parts() {
    let h = head(Some("npub1abc"), None, None);
    assert_eq!(derived_name(&h, &[], &[]), "npub1abc");
}

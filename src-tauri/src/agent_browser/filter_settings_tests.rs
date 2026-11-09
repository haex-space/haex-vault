use super::filter_settings::{CustomList, TrackerSettings, DEFAULT_LISTS};

fn custom(url: &str) -> CustomList {
    CustomList {
        id: String::new(),
        name: String::new(),
        url: url.to_owned(),
        enabled: true,
    }
}

fn with(lists: Vec<CustomList>) -> TrackerSettings {
    TrackerSettings {
        custom: lists,
        ..TrackerSettings::default()
    }
}

#[test]
fn a_new_list_gets_an_id_and_the_host_as_name_and_each_id_is_unique() {
    let kept = with(vec![
        custom("https://lists.example/a.txt"),
        custom("https://other.example/b.txt"),
    ])
    .validated()
    .unwrap();
    assert_eq!(kept.custom[0].name, "lists.example");
    assert_ne!(kept.custom[0].id, kept.custom[1].id);
    let mut clash = kept.clone();
    clash.custom[1].id = clash.custom[0].id.clone();
    assert!(clash.validated().is_err());
    let mut default_id = kept;
    default_id.custom[0].id = DEFAULT_LISTS[0].id.to_owned();
    assert!(
        default_id.validated().is_err(),
        "a default list's id is taken"
    );
}

#[test]
fn only_an_https_address_of_a_sensible_length_is_a_list() {
    for url in [
        "http://lists.example/a.txt",
        "file:///etc/passwd",
        "not an address",
        "",
    ] {
        assert!(with(vec![custom(url)]).validated().is_err(), "{url}");
    }
    let long = format!("https://lists.example/{}", "a".repeat(2100));
    assert!(with(vec![custom(&long)]).validated().is_err());
    let many = (0..51)
        .map(|n| custom(&format!("https://lists.example/{n}.txt")))
        .collect();
    assert!(with(many).validated().is_err());
}

#[test]
fn the_ids_of_default_lists_that_do_not_exist_are_dropped() {
    let mut settings = TrackerSettings::default();
    settings.off.insert("easylist".to_owned());
    settings.off.insert("no-such-list".to_owned());
    let kept = settings.validated().unwrap();
    assert!(kept.off.contains("easylist"));
    assert!(!kept.off.contains("no-such-list"));
}

#[test]
fn an_id_names_a_file_so_it_may_hold_only_letters_digits_and_dashes() {
    for id in ["../x", "a.b", "a b", "settings.json", &"a".repeat(65)] {
        let mut list = custom("https://lists.example/a.txt");
        list.id = id.to_owned();
        assert!(with(vec![list]).validated().is_err(), "{id}");
    }
    let mut ok = custom("https://lists.example/a.txt");
    ok.id = "my-list-2".to_owned();
    assert!(with(vec![ok]).validated().is_ok());
}

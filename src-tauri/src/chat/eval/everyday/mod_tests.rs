use std::collections::BTreeSet;

use serde_json::{json, Value};

use super::{
    embedded_mail_tools, embedded_set, embedded_world, load_set, load_set_file, mail_tools_source,
    Category, Phrasing, MAIL_TOOLS_SOURCE,
};
use crate::chat::eval::scoring::Lang;

fn set_with(requests: Value) -> String {
    json!({ "version": 1, "mailTools": MAIL_TOOLS_SOURCE, "requests": requests }).to_string()
}

fn clean(id: &str, expect: Value) -> Value {
    json!({ "id": id, "lang": "de", "category": "time", "phrasing": "clean", "text": "x", "expect": expect })
}

#[test]
fn the_mail_tools_are_the_pinned_manifest_and_named_as_in_the_app() {
    assert_eq!(mail_tools_source(), MAIL_TOOLS_SOURCE);
    let names: Vec<String> = embedded_mail_tools()
        .into_iter()
        .map(|d| d.model_name)
        .collect();
    assert_eq!(names.len(), 15);
    // After the extension's name, haex-mail, as the app names them.
    assert!(names.iter().all(|n| n.starts_with("x_haexmail_")));
    assert!(names.contains(&"x_haexmail_find_messages".to_owned()));
}

#[test]
fn the_embedded_set_covers_every_category_in_both_languages_with_a_third_spoken() {
    let set = embedded_set();
    assert!(set.requests.len() >= 80, "SC-001: at least 80 requests");
    let spoken = set
        .requests
        .iter()
        .filter(|r| r.phrasing == Phrasing::Spoken)
        .count();
    assert!(spoken * 3 >= set.requests.len(), "FR-004: a third spoken");
    let covered: BTreeSet<(Category, Lang)> =
        set.requests.iter().map(|r| (r.category, r.lang)).collect();
    for category in [
        Category::Time,
        Category::MailRead,
        Category::MailNew,
        Category::MailReply,
        Category::MailTidy,
        Category::Contact,
        Category::Password,
        Category::File,
        Category::AppSetting,
        Category::Clarify,
        Category::NoTool,
    ] {
        for lang in [Lang::De, Lang::En] {
            assert!(covered.contains(&(category, lang)), "{category:?} {lang:?}");
        }
    }
}

#[test]
fn the_world_loads_with_its_fixed_clock() {
    let world = embedded_world();
    let clock = world.clock();
    assert_eq!(
        (clock.now.year, clock.now.month, clock.now.day),
        (2026, 3, 18)
    );
    assert_eq!((clock.now.hour, clock.now.minute), (9, 41));
    assert_eq!(world.day(-1), "2026-03-17");
}

#[test]
fn a_duplicate_id_is_refused() {
    let json = set_with(json!([
        clean("a", json!("refusal")),
        clean("a", json!("refusal"))
    ]));
    assert!(load_set(&json).unwrap_err().contains("duplicate"));
}

#[test]
fn only_a_spoken_request_may_be_misheard() {
    let mut request = clean("a", json!("refusal"));
    request["misheard"] = json!(true);
    assert!(load_set(&set_with(json!([request])))
        .unwrap_err()
        .contains("misheard"));
}

#[test]
fn an_unknown_tool_is_refused() {
    let json = set_with(json!([clean(
        "a",
        json!({ "calls": [{ "tool": "x_alarm_set" }] })
    )]));
    assert!(load_set(&json).unwrap_err().contains("unknown tool"));
}

#[test]
fn a_set_for_another_manifest_revision_is_refused() {
    let json =
        json!({ "version": 1, "mailTools": "haex-space/haextension@0000:x", "requests": [] })
            .to_string();
    assert!(load_set(&json).is_err());
}

#[test]
fn a_spoken_request_needs_a_clean_twin_with_the_same_expectation() {
    let spoken = |twin: Value, expect: Value| json!({ "id": "a-s", "lang": "de", "category": "time", "phrasing": "spoken", "twinOf": twin, "text": "x", "expect": expect });
    let missing = set_with(json!([spoken(Value::Null, json!("refusal"))]));
    assert!(load_set(&missing).is_err());
    let other_expect = set_with(json!([
        clean("a", json!("refusal")),
        spoken(json!("a"), json!("question"))
    ]));
    assert!(load_set(&other_expect).is_err());
    let fine = set_with(json!([
        clean("a", json!("refusal")),
        spoken(json!("a"), json!("refusal"))
    ]));
    assert!(load_set(&fine).is_ok());
}

#[test]
fn a_set_file_loads_from_its_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chainset.json");
    let expect = json!({ "calls": [{ "tool": "x_mail_find_messages" }] });
    std::fs::write(&path, set_with(json!([clean("t-1", expect)]))).unwrap();
    let set = load_set_file(&path).unwrap();
    assert_eq!(set.requests.len(), 1);
    assert_eq!(set.requests[0].id, "t-1");
}

#[test]
fn a_set_file_with_an_unknown_tool_fails_with_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chainset.json");
    let expect = json!({ "calls": [{ "tool": "x_mail_no_such_tool" }] });
    std::fs::write(&path, set_with(json!([clean("t-1", expect)]))).unwrap();
    let error = load_set_file(&path).unwrap_err();
    assert!(error.contains("x_mail_no_such_tool"), "{error}");
}

#[test]
fn a_missing_set_file_fails_with_its_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.json");
    let error = load_set_file(&path).unwrap_err();
    assert!(error.contains("missing.json"), "{error}");
}

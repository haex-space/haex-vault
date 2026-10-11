use super::*;

#[test]
fn every_vector_passes() {
    let checked = check(V1).unwrap();
    assert!(checked > 60, "{checked}");
}

#[test]
fn a_changed_vector_fails_its_check() {
    let vectors: serde_json::Value = serde_json::from_str(V1).unwrap();
    let kek = vectors["keys"]["kek"].as_str().unwrap();
    let changed = V1.replace(kek, &kek.replace(&kek[..2], "00"));
    assert!(check(&changed).unwrap_err().contains("KEK"));
}

#[test]
fn a_content_case_that_is_not_refused_fails_the_check() {
    // Without its ops, "a changed byte in a block" is the intact object.
    let mut vectors: serde_json::Value = serde_json::from_str(V1).unwrap();
    vectors["content_cases"][0]["ops"] = serde_json::json!([]);
    let error = check(&vectors.to_string()).unwrap_err();
    assert!(error.contains("was not refused"), "{error}");
}

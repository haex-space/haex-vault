use super::text;

#[test]
fn a_text_that_names_the_assistant_gets_its_written_name() {
    // The default name; no test here changes the name of the process.
    assert_eq!(
        text("en", "errors.operationBusy", &[]),
        "Haex is still busy with another request. Try again in a moment."
    );
    assert_eq!(
        text("de", "errors.operationBusy", &[]),
        "Haex ist noch mit einer anderen Anfrage beschäftigt. Versuch es gleich noch einmal."
    );
}

#[test]
fn a_missing_key_is_the_key() {
    assert_eq!(text("en", "no.such.key", &[]), "no.such.key");
}

use super::parse_answer;

#[test]
fn the_first_object_is_read_around_a_code_fence() {
    let (text, calls) = parse_answer(
        "```json\n{\"text\": \"Gleich.\", \"calls\": [{\"name\": \"settings_get\", \"arguments\": {}}]}\n```\n{\"text\": \"second\"}",
    );
    assert_eq!(text, "Gleich.");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "settings_get");
}

#[test]
fn a_call_without_a_name_is_kept_for_the_runner_to_reject() {
    let (_, calls) = parse_answer("{\"text\": \"\", \"calls\": [{\"arguments\": {}}, \"x\"]}");
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|c| c.name.is_empty()));
}

#[test]
fn a_reply_without_an_object_is_its_text() {
    let (text, calls) = parse_answer("Es ist 9:41 Uhr.");
    assert_eq!(text, "Es ist 9:41 Uhr.");
    assert!(calls.is_empty());
}

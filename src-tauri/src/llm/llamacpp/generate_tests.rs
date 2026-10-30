use super::take_complete_utf8;

#[test]
fn whole_text_is_returned_and_cleared() {
    let mut bytes = b"hello".to_vec();
    assert_eq!(take_complete_utf8(&mut bytes).as_deref(), Some("hello"));
    assert!(bytes.is_empty());
}

#[test]
fn a_character_cut_between_tokens_waits_for_its_second_half() {
    // "é" is 0xC3 0xA9.
    let mut bytes = vec![b'a', 0xC3];
    assert_eq!(take_complete_utf8(&mut bytes).as_deref(), Some("a"));
    assert_eq!(bytes, vec![0xC3]);
    bytes.push(0xA9);
    assert_eq!(take_complete_utf8(&mut bytes).as_deref(), Some("é"));
    assert!(bytes.is_empty());
}

#[test]
fn only_a_partial_character_gives_nothing_yet() {
    let mut bytes = vec![0xE2, 0x82];
    assert_eq!(take_complete_utf8(&mut bytes), None);
    assert_eq!(bytes, vec![0xE2, 0x82]);
}

#[test]
fn an_invalid_byte_is_replaced_and_the_rest_goes_on() {
    let mut bytes = vec![b'a', 0xFF, b'b'];
    assert_eq!(
        take_complete_utf8(&mut bytes).as_deref(),
        Some("a\u{FFFD}b")
    );
    assert!(bytes.is_empty());
}

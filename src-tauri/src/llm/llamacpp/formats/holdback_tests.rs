use super::Holdback;

const MARKER: &str = "<tool_call>";

#[test]
fn plain_text_is_shown_at_once() {
    let mut hold = Holdback::new(MARKER);
    assert_eq!(hold.push("Hello "), "Hello ");
    assert_eq!(hold.push("world"), "world");
    assert!(!hold.in_call());
    assert_eq!(hold.finish(), "");
}

#[test]
fn a_marker_split_over_chunks_is_held_back_and_never_shown() {
    let mut hold = Holdback::new(MARKER);
    assert_eq!(hold.push("Sure. <tool_c"), "Sure. ");
    assert_eq!(hold.push("all>\n{\"name\": \"a\""), "");
    assert!(hold.in_call());
    assert_eq!(hold.push("}\n</tool_call>"), "");
    assert_eq!(
        hold.finish(),
        "<tool_call>\n{\"name\": \"a\"}\n</tool_call>"
    );
}

#[test]
fn a_whole_marker_in_one_chunk_shows_the_text_before_it() {
    let mut hold = Holdback::new(MARKER);
    assert_eq!(hold.push("OK <tool_call>{}"), "OK ");
    assert!(hold.in_call());
}

#[test]
fn a_prefix_that_never_becomes_the_marker_is_shown_later() {
    let mut hold = Holdback::new(MARKER);
    assert_eq!(hold.push("a <tool"), "a ");
    assert_eq!(hold.push(" b"), "<tool b");
    assert!(!hold.in_call());
    assert_eq!(hold.finish(), "");
}

#[test]
fn an_unfinished_prefix_at_the_end_comes_back_as_text() {
    let mut hold = Holdback::new(MARKER);
    assert_eq!(hold.push("x <tool_ca"), "x ");
    assert!(!hold.in_call());
    assert_eq!(hold.finish(), "<tool_ca");
}

#[test]
fn multi_byte_text_next_to_a_marker_is_not_split() {
    let mut hold = Holdback::new(MARKER);
    assert_eq!(hold.push("Küche <"), "Küche ");
    assert_eq!(hold.push("tool_call>"), "");
    assert!(hold.in_call());
}

#[test]
fn a_multibyte_marker_does_not_slice_inside_its_first_character() {
    let mut hold = Holdback::new("éx");
    assert_eq!(hold.push("a"), "a");
    assert_eq!(hold.push("é"), "");
    assert_eq!(hold.push("x"), "");
    assert!(hold.in_call());
    assert_eq!(hold.finish(), "éx");
}

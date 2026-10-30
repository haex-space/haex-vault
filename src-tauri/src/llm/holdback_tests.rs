use super::longest_prefix_overlap;

#[test]
fn longest_prefix_overlap_finds_the_longest_match() {
    assert_eq!(longest_prefix_overlap("foo <tool_c", "<tool_call>"), 7);
    assert_eq!(longest_prefix_overlap("foo bar", "<tool_call>"), 0);
    assert_eq!(longest_prefix_overlap("<tool_call>", "<tool_call>"), 11);
    assert_eq!(longest_prefix_overlap("", "<tool_call>"), 0);
}

#[test]
fn longest_prefix_overlap_ignores_text_that_ends_inside_a_wider_character() {
    // A marker is ASCII, so a multi-byte character at the end can never be part of an overlap.
    assert_eq!(longest_prefix_overlap("Küche <", "<tool_call>"), 1);
    assert_eq!(longest_prefix_overlap("Küche", "<tool_call>"), 0);
}

#[test]
fn longest_prefix_overlap_skips_marker_offsets_inside_a_wider_character() {
    assert_eq!(longest_prefix_overlap("a", "éx"), 0);
    assert_eq!(longest_prefix_overlap("é", "éx"), "é".len());
}

use super::*;

fn part(start: u64, end: u64) -> Span {
    Span::Part { start, end }
}

// Cases taken over from haex-vault 2.x (Legacy) `remote_storage/streaming/protocol.rs` (revision
// fc4e84b61a050576ba42e0dc832d04064a8605a3), where they still hold.

#[test]
fn no_header_is_the_whole_file() {
    assert_eq!(span(None, 1000), Span::Whole);
}

#[test]
fn a_closed_range() {
    assert_eq!(span(Some("bytes=0-99"), 1000), part(0, 99));
    assert_eq!(span(Some("bytes=0-0"), 1), part(0, 0));
    assert_eq!(span(Some("bytes=0-999"), 1000), part(0, 999));
}

#[test]
fn an_open_ended_range() {
    assert_eq!(span(Some("bytes=500-"), 1000), part(500, 999));
    assert_eq!(span(Some("bytes=0-"), 1000), part(0, 999));
}

#[test]
fn a_suffix_range_takes_the_last_bytes() {
    assert_eq!(span(Some("bytes=-100"), 1000), part(900, 999));
    assert_eq!(span(Some("bytes=-5000"), 1000), part(0, 999));
}

/// Unlike haex-vault 2.x: an end past the file is cut to its last byte (RFC 7233 §2.1).
#[test]
fn an_end_past_the_file_is_cut() {
    assert_eq!(span(Some("bytes=0-1000"), 1000), part(0, 999));
}

#[test]
fn broken_or_unsatisfiable_ranges() {
    for header in [
        "bytes=500-499",
        "bytes=1000-",
        "bytes=0-99,200-299",
        "range=0-99",
        "bytes=abc-99",
        "bytes=0-xyz",
        "bytes=",
        "bytes=-0",
    ] {
        assert_eq!(span(Some(header), 1000), Span::Unsatisfiable, "{header}");
    }
}

#[test]
fn an_empty_file_satisfies_no_range() {
    assert_eq!(span(Some("bytes=0-"), 0), Span::Unsatisfiable);
    assert_eq!(span(None, 0), Span::Whole);
}

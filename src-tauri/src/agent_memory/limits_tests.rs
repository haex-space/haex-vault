//! Tests for the limits of the memory (spec 097 FR-003, research R2, T004).

use super::super::check::Field;
use super::*;

fn text(chars: usize, c: char) -> String {
    std::iter::repeat_n(c, chars).collect()
}

#[test]
fn texts_are_counted_in_characters_not_bytes() {
    for (field, max) in [
        (Field::Title, TITLE_MAX_CHARS),
        (Field::Description, DESCRIPTION_MAX_CHARS),
        (Field::Body, BODY_MAX_CHARS),
    ] {
        for c in ['a', 'ü', '😀'] {
            assert_eq!(fits(field, &text(max, c), None), Ok(()), "{field:?} {c}");
            assert_eq!(
                fits(field, &text(max + 1, c), None),
                Err(LimitError::TooLong(field)),
                "{field:?} {c}"
            );
        }
    }
}

#[test]
fn a_text_over_the_limit_may_stay_or_shrink_but_not_grow() {
    let stored = text(BODY_MAX_CHARS + 50, 'a');
    assert_eq!(fits(Field::Body, &stored, Some(&stored)), Ok(()));
    assert_eq!(
        fits(Field::Body, &text(BODY_MAX_CHARS + 10, 'b'), Some(&stored)),
        Ok(())
    );
    assert_eq!(
        fits(Field::Body, &text(BODY_MAX_CHARS + 51, 'b'), Some(&stored)),
        Err(LimitError::TooLong(Field::Body))
    );
}

#[test]
fn a_new_page_beyond_the_last_is_refused() {
    assert_eq!(room_for_page(MAX_PAGES - 1), Ok(()));
    assert_eq!(room_for_page(MAX_PAGES), Err(LimitError::MemoryFull));
    assert_eq!(
        room_for_page(MAX_PAGES + 5),
        Err(LimitError::MemoryFull),
        "a memory over its limit after a sync takes no new page"
    );
}

#[test]
fn an_addition_may_not_pass_the_length_of_a_page() {
    let stored = text(BODY_MAX_CHARS - 10, 'a');
    let fits_exactly = format!("{stored}{}", text(10, 'b'));
    assert_eq!(appended_fits(&stored, &fits_exactly), Ok(()));
    let over = format!("{stored}{}", text(11, 'b'));
    assert_eq!(appended_fits(&stored, &over), Err(LimitError::PageTooLong));
}

#[test]
fn the_codes_are_those_of_the_contract() {
    assert_eq!(LimitError::TooLong(Field::Title).code(), "too_long");
    assert_eq!(LimitError::MemoryFull.code(), "memory_full");
    assert_eq!(LimitError::PageTooLong.code(), "page_too_long");
}

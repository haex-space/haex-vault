//! The limits of the memory (spec 097 FR-003, research R2). A write that would pass one is refused
//! with a reason; nothing is ever shortened or removed. A change that keeps or reduces a size and
//! every removal always run, so a memory that is over its limit after a sync can be tidied.

use super::check::Field;

#[cfg(test)]
#[path = "limits_tests.rs"]
mod tests;

pub const TITLE_MAX_CHARS: usize = 80;
pub const DESCRIPTION_MAX_CHARS: usize = 200;
pub const BODY_MAX_CHARS: usize = 10_000;
pub const MAX_PAGES: usize = 1_000;

/// Why a write passes a limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LimitError {
    #[error("the {} is too long", .0.as_str())]
    TooLong(Field),
    #[error("the memory is full")]
    MemoryFull,
    #[error("the page would be too long")]
    PageTooLong,
}

impl LimitError {
    /// The code of the contract.
    pub fn code(self) -> &'static str {
        match self {
            Self::TooLong(_) => "too_long",
            Self::MemoryFull => "memory_full",
            Self::PageTooLong => "page_too_long",
        }
    }
}

/// The most characters `field` may have.
pub fn max_chars(field: Field) -> usize {
    match field {
        Field::Title => TITLE_MAX_CHARS,
        Field::Description => DESCRIPTION_MAX_CHARS,
        Field::Body => BODY_MAX_CHARS,
    }
}

/// `text` fits `field`, or does not grow beyond `stored`, the text it replaces.
pub fn fits(field: Field, text: &str, stored: Option<&str>) -> Result<(), LimitError> {
    let chars = text.chars().count();
    let before = stored.map_or(0, |stored| stored.chars().count());
    if chars <= max_chars(field) || chars <= before {
        return Ok(());
    }
    Err(LimitError::TooLong(field))
}

/// A new page may be added to a memory of `pages` pages.
pub fn room_for_page(pages: usize) -> Result<(), LimitError> {
    if pages < MAX_PAGES {
        Ok(())
    } else {
        Err(LimitError::MemoryFull)
    }
}

/// The text of a page after an addition (`memory.add`) still fits.
pub fn appended_fits(stored: &str, appended: &str) -> Result<(), LimitError> {
    fits(Field::Body, appended, Some(stored)).map_err(|_| LimitError::PageTooLong)
}

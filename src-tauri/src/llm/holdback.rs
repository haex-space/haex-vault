//! Holding back streamed text that may still turn into a marker.
//!
//! Both local engines stream text and must not show the start of a tool-call tag to the caller
//! before it is clear whether the tag is real; this is the one place that decides how many trailing
//! bytes of a text could still grow into a marker.

/// Longest suffix of `text` that is also a prefix of `marker`: how many trailing bytes of `text` could
/// still grow into `marker` if the next chunk continues it.
pub(crate) fn longest_prefix_overlap(text: &str, marker: &str) -> usize {
    let max = marker.len().min(text.len());
    (1..=max)
        .rev()
        .find(|&len| text.is_char_boundary(text.len() - len) && text.ends_with(&marker[..len]))
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "holdback_tests.rs"]
mod tests;

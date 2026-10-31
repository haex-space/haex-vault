//! Streams text to the caller but keeps back everything from the first character of a tool-call marker on.

use crate::llm::holdback::longest_prefix_overlap;

/// Feeds text in as it is generated and hands back what is safe to show. From the first character of the
/// marker on, nothing is shown; the call block is read from the whole text when the answer ends.
pub struct Holdback {
    markers: Vec<&'static str>,
    pending: String,
    in_call: bool,
}

impl Holdback {
    pub fn new(marker: &'static str) -> Self {
        Self::with_markers(&[marker])
    }

    /// Holds back from the first character of any of `markers` on.
    pub fn with_markers(markers: &[&'static str]) -> Self {
        Self {
            markers: markers.to_vec(),
            pending: String::new(),
            in_call: false,
        }
    }

    /// Takes the next chunk; returns the part that can no longer turn into a marker.
    pub fn push(&mut self, chunk: &str) -> String {
        self.pending.push_str(chunk);
        if self.in_call {
            return String::new();
        }
        if let Some(at) = self
            .markers
            .iter()
            .filter_map(|m| self.pending.find(m))
            .min()
        {
            self.in_call = true;
            let shown = self.pending[..at].to_owned();
            self.pending.drain(..at);
            return shown;
        }
        // The tail may be the start of a marker that the next chunk completes: keep it back.
        let keep = self
            .markers
            .iter()
            .map(|m| longest_prefix_overlap(&self.pending, m))
            .max()
            .unwrap_or(0);
        let safe = self.pending.len() - keep;
        let shown = self.pending[..safe].to_owned();
        self.pending.drain(..safe);
        shown
    }

    /// True once a marker has been seen.
    pub fn in_call(&self) -> bool {
        self.in_call
    }

    /// The end of the answer: what is still held back. Inside a call this is the (maybe unfinished) call
    /// block, which the caller reads through `ChatFormat::parse`; otherwise it is a marker-like prefix that
    /// never became a marker, and is ordinary text.
    pub fn finish(self) -> String {
        self.pending
    }
}

#[cfg(test)]
#[path = "holdback_tests.rs"]
mod tests;

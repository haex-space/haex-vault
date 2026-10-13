//! What this device changed in an open folder's view (research R7): a reload that started before a
//! write would otherwise drop it from the view until the next load, and a sub folder written after
//! it would then lose its parent ("Wiederhergestellt"). [`super::OpenFolder::reload`] applies the
//! changes made since its load began on top of the new load.

use std::collections::VecDeque;

use super::format::Id;
use super::tree::Version;

/// How many changes are kept; a reload takes far fewer than this to load.
const KEPT: usize = 4096;

/// One change of this device.
#[derive(Clone)]
pub enum Change {
    /// A version written, with its content object and length.
    Add(Box<Version>, Option<(Id, u64)>),
    /// Versions deleted.
    Remove(Vec<Id>),
}

/// The changes of this device, numbered.
#[derive(Clone, Default)]
pub struct Journal {
    /// The number of the last change (0: none yet).
    pub mark: u64,
    changes: VecDeque<(u64, Change)>,
}

impl Journal {
    pub fn record(&mut self, change: Change) {
        self.mark += 1;
        self.changes.push_back((self.mark, change));
        if self.changes.len() > KEPT {
            self.changes.pop_front();
        }
    }

    /// The changes after `mark`, oldest first.
    pub fn since(&self, mark: u64) -> impl Iterator<Item = &Change> {
        self.changes
            .iter()
            .filter(move |(at, _)| *at > mark)
            .map(|(_, change)| change)
    }
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;

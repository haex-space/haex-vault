use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Get the current Unix timestamp in seconds.
pub(super) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Speed tracker — sliding window bytes/second
// ---------------------------------------------------------------------------

pub(super) struct SpeedTracker {
    samples: VecDeque<(Instant, u64)>,
}

impl SpeedTracker {
    pub(super) fn new() -> Self {
        Self {
            samples: VecDeque::new(),
        }
    }

    pub(super) fn add(&mut self, bytes: u64) {
        self.samples.push_back((Instant::now(), bytes));
        let cutoff = Instant::now() - Duration::from_secs(5);
        while self
            .samples
            .front()
            .map(|(t, _)| *t < cutoff)
            .unwrap_or(false)
        {
            self.samples.pop_front();
        }
    }

    pub(super) fn bytes_per_second(&self) -> u64 {
        if self.samples.len() < 2 {
            return 0;
        }
        let oldest = self
            .samples
            .front()
            .expect("invariant: samples.len() >= 2 checked above")
            .0;
        let newest = self
            .samples
            .back()
            .expect("invariant: samples.len() >= 2 checked above")
            .0;
        let elapsed = newest.duration_since(oldest).as_secs_f64();
        let total: u64 = self.samples.iter().map(|(_, b)| b).sum();
        if elapsed < 0.05 {
            return 0;
        }
        (total as f64 / elapsed) as u64
    }
}

//! Which compute units local models are computing on right now (spec 081 FR-016, operator
//! 2026-11-06): the local chat model claims the units it runs on for each model call, and speech
//! output waits before each sentence and each part until its own unit is free. Two models that
//! share the processor or a graphics card slow each other down far more than they gain by running
//! side by side (measured: Qwen3-TTS took 10 s per second of speech next to the chat model, 1.35 s
//! alone), so the chat model computes first and speech follows. Units the chat model does not use
//! stay free: a chat model on the graphics card, or one of a provider, lets speech run alongside.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Where a local model computes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Processor,
    GraphicsCard,
}

/// How often a waiting speech output looks again.
const POLL: Duration = Duration::from_millis(20);

/// Counts the claims per unit.
#[derive(Debug, Default)]
pub struct ComputeUse {
    processor: AtomicUsize,
    graphics_card: AtomicUsize,
}

/// The process's compute use: one vault per process (spec 013), so one set of units.
pub static COMPUTE: ComputeUse = ComputeUse::new();

impl ComputeUse {
    pub const fn new() -> Self {
        Self {
            processor: AtomicUsize::new(0),
            graphics_card: AtomicUsize::new(0),
        }
    }

    fn counter(&self, unit: Unit) -> &AtomicUsize {
        match unit {
            Unit::Processor => &self.processor,
            Unit::GraphicsCard => &self.graphics_card,
        }
    }

    /// Claims `units` until the returned claim is dropped.
    pub fn claim<'a>(&'a self, units: &[Unit]) -> Claim<'a> {
        for unit in units {
            self.counter(*unit).fetch_add(1, Ordering::AcqRel);
        }
        Claim {
            owner: self,
            units: units.to_vec(),
        }
    }

    /// Whether something claims `unit` now.
    pub fn busy(&self, unit: Unit) -> bool {
        self.counter(unit).load(Ordering::Acquire) > 0
    }

    /// Blocks while `unit` is claimed. Returns `false` when `cancelled` said so while waiting.
    /// Call it off the async runtime.
    pub fn wait_free(&self, unit: Unit, cancelled: &dyn Fn() -> bool) -> bool {
        while self.busy(unit) {
            if cancelled() {
                return false;
            }
            std::thread::sleep(POLL);
        }
        !cancelled()
    }
}

/// Holds units until dropped.
#[derive(Debug)]
pub struct Claim<'a> {
    owner: &'a ComputeUse,
    units: Vec<Unit>,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        for unit in &self.units {
            self.owner.counter(*unit).fetch_sub(1, Ordering::AcqRel);
        }
    }
}

/// The units a llama.cpp model computes on, from how many of its layers are on a graphics card
/// (`gpu_layers`), how many layers it has, and whether there is a graphics card at all: none on the
/// card is the processor only, all of them (layers plus the output layer) the card only, and a
/// split both.
pub fn units_of_layers(gpu_layers: u32, layers: u32, graphics_card: bool) -> Vec<Unit> {
    if !graphics_card || gpu_layers == 0 {
        vec![Unit::Processor]
    } else if gpu_layers > layers {
        vec![Unit::GraphicsCard]
    } else {
        vec![Unit::Processor, Unit::GraphicsCard]
    }
}

#[cfg(test)]
#[path = "compute_tests.rs"]
mod tests;

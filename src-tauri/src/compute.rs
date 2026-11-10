//! Which compute units local models are computing on right now (spec 081 FR-016, operator
//! 2026-11-06): the local chat model claims the units it runs on for each model call, and speech
//! output waits before each sentence and each part until its own unit is free. Two models that
//! share the processor or a graphics card slow each other down far more than they gain by running
//! side by side (measured: Qwen3-TTS took 10 s per second of speech next to the chat model, 1.35 s
//! alone), so the chat model computes first and speech follows. Units the chat model does not use
//! stay free: a chat model on the graphics card, or one of a provider, lets speech run alongside.

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};
use std::time::Duration;

/// Where a local model computes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Processor,
    GraphicsCard,
}

/// How often a waiting speech output looks again.
const POLL: Duration = Duration::from_millis(20);

/// Chat calls share a unit with other chat calls. Speech takes it exclusively, so neither can
/// begin while the other is computing.
#[derive(Debug, Default)]
pub struct ComputeUse {
    processor: RwLock<()>,
    graphics_card: RwLock<()>,
}

/// The process's compute use: one vault per process (spec 013), so one set of units.
pub static COMPUTE: ComputeUse = ComputeUse::new();

impl ComputeUse {
    pub const fn new() -> Self {
        Self {
            processor: RwLock::new(()),
            graphics_card: RwLock::new(()),
        }
    }

    fn lock(&self, unit: Unit) -> &RwLock<()> {
        match unit {
            Unit::Processor => &self.processor,
            Unit::GraphicsCard => &self.graphics_card,
        }
    }

    /// Claims `units` for a chat call until the returned claim is dropped. Calls share read locks;
    /// speech takes a write lock and excludes every chat call on that unit.
    pub fn claim<'a>(&'a self, units: &[Unit]) -> Claim<'a> {
        let mut units = units.to_vec();
        units.sort_unstable_by_key(|unit| match unit {
            Unit::Processor => 0,
            Unit::GraphicsCard => 1,
        });
        units.dedup();
        let guards = units
            .into_iter()
            .map(|unit| self.lock(unit).read().unwrap_or_else(|e| e.into_inner()))
            .map(|_guard| ClaimGuard::Chat { _guard })
            .collect();
        Claim { _guards: guards }
    }

    /// Exclusively claims `unit` for speech. Polling keeps stop/cancellation responsive while it
    /// waits and the returned guard closes the check-then-compute race with chat calls.
    pub fn claim_speech<'a>(
        &'a self,
        unit: Unit,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Claim<'a>> {
        let lock = self.lock(unit);
        loop {
            if cancelled() {
                return None;
            }
            match lock.try_write() {
                Ok(guard) if cancelled() => {
                    drop(guard);
                    return None;
                }
                Ok(guard) => {
                    return Some(Claim {
                        _guards: vec![ClaimGuard::Speech { _guard: guard }],
                    })
                }
                Err(TryLockError::WouldBlock) => std::thread::sleep(POLL),
                Err(TryLockError::Poisoned(error)) => {
                    let guard = error.into_inner();
                    if cancelled() {
                        drop(guard);
                        return None;
                    }
                    return Some(Claim {
                        _guards: vec![ClaimGuard::Speech { _guard: guard }],
                    });
                }
            }
        }
    }

    /// Whether a chat or speech claim holds `unit` now; mainly for diagnostics/tests.
    pub fn busy(&self, unit: Unit) -> bool {
        match self.lock(unit).try_write() {
            Ok(guard) => {
                drop(guard);
                false
            }
            Err(TryLockError::WouldBlock) => true,
            Err(TryLockError::Poisoned(_)) => true,
        }
    }
}

#[derive(Debug)]
enum ClaimGuard<'a> {
    Chat { _guard: RwLockReadGuard<'a, ()> },
    Speech { _guard: RwLockWriteGuard<'a, ()> },
}

/// Holds compute-unit locks until dropped.
#[derive(Debug)]
pub struct Claim<'a> {
    _guards: Vec<ClaimGuard<'a>>,
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

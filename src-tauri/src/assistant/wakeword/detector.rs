//! The wake word over a stream of 16 kHz audio (T066), free of models so tests drive it with a
//! fake [`Scorer`].
//!
//! The embedding model puts 11,120 zeros in front of its input and returns one frame per 1,280
//! samples; frame `j` sees samples `[j·1280 − 11120, (j+1)·1280)`. Fed the last 10 · 1,280
//! samples, its last frame sees only real audio: that is one run-time embedding. haex computes one
//! per 80 ms while the pause detection hears speech (and a moment after). When it starts hearing
//! speech, the 16 frames before are computed in one call over the last 25 · 1,280 samples, so the
//! "Hey" that came before the detection counts too.
//!
//! haex's own output never wakes it (FR-010, T067). The watcher runs only while haex is quiet, and
//! the echo cancellation takes what haex plays out of what the microphone hears. What can still
//! reach the detector is a rest of it: the reverb of the last sentence, a sample or a read-aloud
//! chat message while haex waits, or a stream without echo cancellation. Each hop knows whether
//! haex was audible ([`Echo`]): with own output in the window a stricter rule applies, and a
//! detection is ignored when own output that nothing cancelled falls where the wake word is, the
//! last [`WORD_HOPS`] of the window. (T067 said "more than half of the window"; the word sits at
//! its end, so haex saying "Hex" fills only about a third of the window and would pass.)

use std::collections::VecDeque;

use super::Rule;

/// Samples per embedding (80 ms at 16 kHz).
pub const HOP: usize = 1280;
/// Embeddings the classifier sees (1.28 s).
pub const WINDOW: usize = 16;
/// Values per embedding.
pub const EMBEDDING: usize = 96;
/// The first frame of a call that sees no padding.
pub const FIRST_REAL: usize = 9;
/// Samples for one run-time embedding.
pub const STREAM_SAMPLES: usize = (FIRST_REAL + 1) * HOP;
/// Samples for a full window at once.
pub const BACKFILL_SAMPLES: usize = (FIRST_REAL + WINDOW) * HOP;
/// After the detection fired, 2 s without another.
const REFRACTORY_HOPS: u64 = 25;
/// Hops the detector keeps running after the pause detection stopped hearing speech: the wake
/// word ends a few frames before the classifier is surest (0.4 s).
const HANGOVER_HOPS: u32 = 5;
/// The end of the window where the wake word is when the classifier fires (0.64 s, "Hey Hex").
const WORD_HOPS: usize = 8;
/// The rule while haex's own output is in the window: at least as strict as "low", the strictest
/// sensitivity (3 windows), at the threshold of "medium" (0.98).
const STRICT: Rule = Rule {
    windows: 3,
    threshold: 0.98,
};

/// What haex itself played while a block was heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Echo {
    /// haex was silent.
    #[default]
    None,
    /// haex played, and the echo cancellation takes it out of what the microphone hears.
    Cancelled,
    /// haex played, and the microphone hears it as it is.
    Raw,
}

impl Rule {
    /// This rule, made at least as strict as [`STRICT`].
    fn strict(self) -> Self {
        Self {
            windows: self.windows.max(STRICT.windows),
            threshold: self.threshold.max(STRICT.threshold),
        }
    }
}

/// The two models.
pub trait Scorer {
    /// The embeddings of `audio` from the first frame that sees no padding on
    /// (`audio.len() / 1280 − 9` of them).
    fn embed(&mut self, audio: &[f32]) -> Vec<[f32; EMBEDDING]>;
    /// The classifier's probability for one window of [`WINDOW`] embeddings.
    fn score(&mut self, window: &[[f32; EMBEDDING]]) -> f32;
}

pub struct Detector<S> {
    scorer: S,
    rule: Rule,
    /// The last [`BACKFILL_SAMPLES`] samples.
    ring: VecDeque<f32>,
    /// Samples of the hop being filled.
    pending: usize,
    frames: VecDeque<[f32; EMBEDDING]>,
    /// `frames` ends with the embedding of the last hop.
    current: bool,
    hops: u64,
    hangover: u32,
    run: usize,
    quiet_until: u64,
    /// What haex played during the last [`WINDOW`] hops.
    echoes: VecDeque<Echo>,
}

impl<S: Scorer> Detector<S> {
    pub fn new(scorer: S, rule: Rule) -> Self {
        Self {
            scorer,
            rule,
            // Silence before the first sample: a wake word said right after the watcher started
            // has the 2 s of audio the backfill needs, as after any quiet stretch.
            ring: std::iter::repeat_n(0.0, BACKFILL_SAMPLES).collect(),
            pending: 0,
            frames: VecDeque::with_capacity(WINDOW + 1),
            current: false,
            hops: 0,
            hangover: 0,
            run: 0,
            quiet_until: 0,
            echoes: VecDeque::with_capacity(WINDOW + 1),
        }
    }

    pub fn set_rule(&mut self, rule: Rule) {
        self.rule = rule;
    }

    /// Takes `block` (16 kHz mono); `speech` is whether the pause detection hears speech at its
    /// end, `echo` what haex played meanwhile. Returns whether the wake word was said.
    pub fn push(&mut self, block: &[f32], speech: bool, echo: Echo) -> bool {
        let mut fired = false;
        for chunk in block.chunks(HOP) {
            let mut rest = chunk;
            while !rest.is_empty() {
                let take = rest.len().min(HOP - self.pending);
                self.ring.extend(&rest[..take]);
                while self.ring.len() > BACKFILL_SAMPLES {
                    self.ring.pop_front();
                }
                self.pending += take;
                rest = &rest[take..];
                if self.pending == HOP {
                    self.pending = 0;
                    fired |= self.hop(speech, echo);
                }
            }
        }
        fired
    }

    fn hop(&mut self, speech: bool, echo: Echo) -> bool {
        self.hops += 1;
        self.echoes.push_back(echo);
        while self.echoes.len() > WINDOW {
            self.echoes.pop_front();
        }
        if speech {
            self.hangover = HANGOVER_HOPS;
        } else if self.hangover > 0 {
            self.hangover -= 1;
        } else {
            self.current = false;
            self.run = 0;
            return false;
        }
        if self.current {
            let tail: Vec<f32> = self
                .ring
                .range(self.ring.len() - STREAM_SAMPLES..)
                .copied()
                .collect();
            let frame = self.scorer.embed(&tail);
            let Some(last) = frame.last() else {
                return false;
            };
            self.frames.push_back(*last);
            while self.frames.len() > WINDOW {
                self.frames.pop_front();
            }
        } else {
            if self.ring.len() < BACKFILL_SAMPLES {
                return false;
            }
            let all: Vec<f32> = self.ring.iter().copied().collect();
            self.frames = self.scorer.embed(&all).into_iter().collect();
            while self.frames.len() > WINDOW {
                self.frames.pop_front();
            }
            self.current = true;
        }
        if self.frames.len() < WINDOW {
            return false;
        }
        let rule = if self.echoes.iter().any(|echo| *echo != Echo::None) {
            self.rule.strict()
        } else {
            self.rule
        };
        let window: Vec<[f32; EMBEDDING]> = self.frames.iter().copied().collect();
        if self.scorer.score(&window) >= rule.threshold {
            self.run += 1;
        } else {
            self.run = 0;
        }
        if self.run >= rule.windows && self.hops > self.quiet_until {
            self.run = 0;
            let word = self.echoes.iter().rev().take(WORD_HOPS);
            if word.into_iter().any(|echo| *echo == Echo::Raw) {
                // haex itself, as the microphone heard it, where the wake word is.
                return false;
            }
            self.quiet_until = self.hops + REFRACTORY_HOPS;
            return true;
        }
        false
    }
}

#[cfg(test)]
#[path = "detector_tests.rs"]
mod tests;

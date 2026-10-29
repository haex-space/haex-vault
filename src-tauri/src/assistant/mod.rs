//! The voice assistant (spec 066): wake word, spoken conversation and speech output over the
//! built-in agent. Gated by the `voice` feature like `audio`.

pub mod vad;
#[cfg(test)]
mod vad_tests;
